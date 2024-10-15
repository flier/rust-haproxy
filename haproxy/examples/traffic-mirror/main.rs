//! Replicating (mirroring) HTTP requests using the HAProxy SPOP,
//! i.e. Stream Processing Offload Protocol.
//!
//! This is a very simple program that can be used to replicate HTTP requests
//! via the SPOP protocol.  All requests are replicated to the web address (URL)
//! selected when running the program.

use core::str;
use std::env;
use std::fs::create_dir_all;
use std::io;
use std::net::IpAddr;
use std::path::PathBuf;
use std::{convert::Infallible, fs::File};

use anyhow::{bail, Context, Result};
use bytes::Buf;
use clap::Parser;
use daemonize::Daemonize;
use haproxy_spop::Scope;
use humantime::Duration;
use rand::{thread_rng, Rng};
use reqwest::{
    header::HeaderMap, Body, Client, ClientBuilder, Method, RequestBuilder, Url, Version,
};
use rlimit::{getrlimit, setrlimit, Resource};
use tokio::signal;
use tokio::task::JoinSet;
use tower::service_fn;
use tracing::{debug, instrument, trace};
use tracing_subscriber::prelude::*;

use haproxy::{
    agent::{req, runtime, Agent},
    proto::{Action, Capability, Message, Typed, MAX_FRAME_SIZE},
};

#[derive(Debug, Parser)]
#[command(version, author, about)]
struct Opt {
    /// Specify the address to listen on
    #[arg(short, long, default_value = "127.0.0.1")]
    addr: String,

    /// Specify the port to listen on
    #[arg(short, long, default_value = "12345")]
    port: u16,

    /// Specify the connection backlog size
    #[arg(short, long, default_value_t = 10)]
    backlog: i32,

    /// Specify the number of workers
    #[arg(short, long)]
    num_workers: Option<usize>,

    /// Enable the support of the specified capability.
    #[arg(short, long, value_enum, default_values_t = [Capability::Pipelining])]
    capability: Vec<Capability>,

    /// Specify the maximum frame size
    #[arg(short, long, default_value_t = MAX_FRAME_SIZE)]
    max_frame_size: usize,

    /// Set a delay to process a message
    #[arg(short = 't', long, default_value_t = runtime::MAX_PROCESS_TIME.into())]
    processing_delay: Duration,

    /// Run this program as a daemon.
    #[arg(short = 'D', long)]
    daemonize: bool,

    /// Specifies a file to write the process-id to.
    #[arg(short = 'F', long)]
    pid_file: Option<PathBuf>,

    /// Change root directory
    #[arg(long)]
    chroot: Option<PathBuf>,

    /// Specify the URL for the HTTP mirroring.
    #[arg(short = 'u', long)]
    mirror_url: String,
}

pub fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(console_subscriber::spawn())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let opt = Opt::parse();
    debug!(?opt);

    let runtime = {
        runtime::Builder::new()
            .capabilities(opt.capability)
            .max_frame_size(opt.max_frame_size)
            .max_process_time(opt.processing_delay)
            .make_service(
                service_fn(|(client, base): (Client, Url)| async move {
                    Ok::<_, Infallible>(service_fn(move |msgs: Vec<Message>| {
                        process_request(client.clone(), base.clone(), msgs)
                    }))
                }),
                (
                    ClientBuilder::new().build()?,
                    opt.mirror_url.parse::<Url>()?,
                ),
            )
    };
    let listener = {
        let Opt {
            addr,
            port,
            backlog,
            ..
        } = opt;

        let listen = move || {
            net2::TcpBuilder::new_v4()?
                .reuse_address(true)?
                .bind((addr, port))?
                .listen(backlog)
        };

        if opt.daemonize {
            daemonize(listen, opt.pid_file, opt.chroot)?
        } else {
            listen()?
        }
    };

    rlimit_setnofile()?;

    let rt: tokio::runtime::Runtime = {
        let mut b = tokio::runtime::Builder::new_multi_thread();
        if let Some(n) = opt.num_workers {
            b.worker_threads(n);
        }
        b.thread_name("worker").enable_all().build()?
    };

    rt.block_on(async move {
        let agent = Agent::new(runtime, listener)?;
        let serve = agent.shutdown();

        tokio::spawn(async move {
            signal::ctrl_c().await.unwrap();

            debug!("received Ctrl+C");

            serve.cancel();
        });

        agent.serve().await
    })?;

    Ok(())
}

#[instrument(skip_all, err)]
fn daemonize<F, T>(action: F, pid_file: Option<PathBuf>, chroot: Option<PathBuf>) -> Result<T>
where
    F: FnOnce() -> io::Result<T> + 'static,
{
    let bin_name: &str = env!("CARGO_BIN_NAME");
    let root_dir = env::temp_dir().join(bin_name);
    create_dir_all(&root_dir)?;

    let pid_file = pid_file.unwrap_or_else(|| root_dir.join(format!("{bin_name}.pid")));
    let stdout = File::create(root_dir.join(format!("{bin_name}.stdout")))?;
    let stderr = File::create(root_dir.join(format!("{bin_name}.stderr")))?;

    let mut daemonize = Daemonize::new()
        .pid_file(pid_file)
        .chown_pid_file(true)
        .umask(0)
        .working_directory(&root_dir)
        .stdout(stdout)
        .stderr(stderr)
        .privileged_action(action);

    if let Some(path) = chroot {
        daemonize = daemonize.chroot(path);
    }

    debug!(?daemonize);

    daemonize.start().context("daemonize")?.context("listen")
}

fn rlimit_setnofile() -> Result<()> {
    let (sort, hard) = getrlimit(Resource::NOFILE)?;
    setrlimit(Resource::NOFILE, hard, hard)?;

    debug!(from=?sort, to=?hard, "setnofile");

    Ok(())
}

#[instrument(skip(client), ret, err, level = "trace")]
async fn process_request(client: Client, base: Url, msgs: Vec<Message>) -> Result<Vec<Action>> {
    let mut actions = Vec::new();
    let mut tasks = JoinSet::new();

    for msg in msgs {
        match msg.name.as_str() {
            "check-client-ip" => {
                actions.push(iprep(msg)?);
            }
            "test" => {
                debug!(%msg.name, ?msg.args);
            }
            "mirror" => {
                mirror(&mut tasks, &client, &base, msg)?;
            }
            msg => debug!(msg, "ignored"),
        }
    }

    if !tasks.is_empty() {
        actions.extend(tasks.join_all().await);
    }

    Ok(actions)
}

fn iprep(msg: Message) -> Result<Action> {
    let addr = msg
        .args
        .into_iter()
        .find(|(name, _)| name == "ip")
        .and_then(|(_, value)| match value {
            Typed::Ipv4(addr) => Some(IpAddr::from(addr)),
            Typed::Ipv6(addr) => Some(IpAddr::from(addr)),
            _ => None,
        });

    if let Some(addr) = addr {
        let score = thread_rng().gen_range(0..=100u32);

        trace!(%addr, score, "IP reputation");

        Ok(Action::set_var(Scope::Session, "ip_score", score))
    } else {
        bail!("missing `ip` argument");
    }
}

fn mirror(tasks: &mut JoinSet<Action>, client: &Client, base: &Url, msg: Message) -> Result<()> {
    for (arg, value) in msg.args {
        let mut builder = Builder::new(base.clone());

        match (arg.as_str(), value) {
            ("arg_method", Typed::String(method)) => {
                builder.method(method);
            }
            ("arg_path", Typed::String(path)) => {
                builder.path(path);
            }
            ("arg_query", Typed::String(query)) if !query.is_empty() => {
                builder.query(query);
            }
            ("arg_ver", Typed::String(version)) => {
                builder.version(version);
            }
            ("arg_hdrs", Typed::Binary(hdrs)) => {
                builder.headers(hdrs);
            }
            ("arg_body", Typed::Binary(body)) if !body.is_empty() => {
                builder.body(body);
            }
            _ => trace!(%arg, "ignored"),
        }

        let req = builder.build(client.clone());

        tasks.build_task().name(&msg.name).spawn(async {
            // let res = req.send().await?;

            Action::set_var(Scope::Session, "foo", "bar")
        })?;
    }

    Ok(())
}

#[derive(Debug)]
pub struct Builder<T> {
    url: Url,
    method: Option<Method>,
    version: Option<Version>,
    headers: HeaderMap,
    body: Option<T>,
}

impl<T> Builder<T> {
    pub fn new(base: Url) -> Self {
        Self {
            url: base,
            method: None,
            version: None,
            headers: HeaderMap::new(),
            body: None,
        }
    }

    pub fn method<S: AsRef<str>>(&mut self, method: S) -> &mut Self {
        self.method = method.as_ref().parse().ok();
        self
    }

    pub fn path<S: AsRef<str>>(&mut self, path: S) -> &mut Self {
        self.url.set_path(path.as_ref());
        self
    }

    pub fn query<S: AsRef<str>>(&mut self, query: S) -> &mut Self {
        self.url.set_query(Some(query.as_ref()));
        self
    }

    pub fn version<S: AsRef<str>>(&mut self, version: S) -> &mut Self {
        self.version = Some(match version.as_ref() {
            "1.0" => Version::HTTP_10,
            "1.1" => Version::HTTP_11,
            "2.0" => Version::HTTP_2,
            "3.0" => Version::HTTP_3,
            v => panic!("unexpected http version {v}"),
        });
        self
    }

    pub fn headers<B: Buf>(&mut self, b: B) -> &mut Self {
        if let Some(hdrs) = req::hdrs_bin(b).ok() {
            self.headers.extend(hdrs);
        }
        self
    }

    pub fn body(&mut self, b: T) -> &mut Self {
        self.body = Some(b);
        self
    }
}

impl<T> Builder<T>
where
    T: Into<Body>,
{
    pub fn build(self, client: Client) -> RequestBuilder {
        let mut builder = client
            .request(self.method.unwrap_or(Method::GET), self.url)
            .version(self.version.unwrap_or(Version::HTTP_11))
            .headers(self.headers);

        if let Some(body) = self.body {
            builder = builder.body(body);
        }

        builder
    }
}
