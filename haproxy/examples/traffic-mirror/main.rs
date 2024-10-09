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
use std::path::PathBuf;
use std::result::Result as StdResult;
use std::{convert::Infallible, fs::File};

use anyhow::{Context, Result};
use bytes::{Buf, Bytes};
use clap::Parser;
use daemonize::Daemonize;
use http::Request;
use humantime::Duration;
use reqwest::Version;
use rlimit::{getrlimit, setrlimit, Resource};
use tokio::signal;
use tower::service_fn;
use tracing::{debug, instrument};
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
                service_fn(|_: ()| async {
                    Ok::<_, Infallible>(service_fn(|msgs: Vec<Message>| process_request(msgs)))
                }),
                (),
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
        let shutdown = agent.shutdown();

        tokio::task::Builder::new()
            .name("singal")
            .spawn(async move {
                signal::ctrl_c().await.unwrap();

                debug!("received Ctrl+C");

                shutdown.shutdown();
            })?;

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

async fn process_request(msgs: Vec<Message>) -> StdResult<Vec<Action>, haproxy::agent::Error> {
    for msg in msgs.into_iter().filter(|msg| msg.name == "mirror") {
        for (arg, value) in msg.args {
            let mut builder = Builder::new();

            match (arg.as_str(), value) {
                ("arg_method", Typed::String(method)) => {
                    builder.method(method);
                }
                ("arg_pathq", Typed::String(path)) => {
                    builder.path(path);
                }
                ("arg_ver", Typed::String(version)) => {
                    builder.version(version);
                }
                ("arg_hdrs", Typed::Binary(hdrs)) => {
                    builder.headers(hdrs)?;
                }
                ("arg_body", Typed::Binary(body)) => {
                    builder.body(body);
                }
                _ => debug!(%arg, "ignored"),
            }

            let req = builder.build()?;
        }
    }

    Ok(vec![])
}

pub struct Builder {
    req: Option<http::request::Builder>,
    body: Option<Bytes>,
}

impl Builder {
    pub fn new() -> Builder {
        Builder {
            req: Some(http::request::Builder::new()),
            body: None,
        }
    }

    pub fn method<S: AsRef<str>>(&mut self, method: S) -> &mut Self {
        self.req = self.req.take().map(|b| b.method(method.as_ref()));
        self
    }

    pub fn path<S: AsRef<str>>(&mut self, path: S) -> &mut Self {
        self.req = self.req.take().map(|b| b.uri(path.as_ref()));
        self
    }

    pub fn version<S: AsRef<str>>(&mut self, version: S) -> &mut Self {
        self.req = self.req.take().map(|b| {
            b.version(match version.as_ref() {
                "1.0" => Version::HTTP_10,
                "1.1" => Version::HTTP_11,
                "2.0" => Version::HTTP_2,
                "3.0" => Version::HTTP_3,
                v => panic!("unexpected http version {v}"),
            })
        });
        self
    }

    pub fn headers<T: Buf>(&mut self, b: T) -> StdResult<&mut Self, haproxy::agent::Error> {
        if let Some(hdrs) = self.req.as_mut().and_then(|b| b.headers_mut()) {
            hdrs.extend(req::hdrs_bin(b)?)
        };

        Ok(self)
    }

    pub fn body(&mut self, b: Bytes) -> &mut Self {
        self.body = Some(b);
        self
    }

    pub fn build(mut self) -> StdResult<Request<Bytes>, haproxy::agent::Error> {
        Ok(self
            .req
            .take()
            .expect("request builder")
            .body(self.body.unwrap_or_default())?)
    }
}
