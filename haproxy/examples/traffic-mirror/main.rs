//! Replicating (mirroring) HTTP requests using the HAProxy SPOP,
//! i.e. Stream Processing Offload Protocol.
//!
//! This is a very simple program that can be used to replicate HTTP requests
//! via the SPOP protocol.  All requests are replicated to the web address (URL)
//! selected when running the program.

use std::env;
use std::fs::create_dir_all;
use std::io;
use std::net::TcpListener;
use std::path::PathBuf;
use std::{convert::Infallible, fs::File};

use anyhow::{Context, Result};
use clap::Parser;
use daemonize::Daemonize;
use rlimit::{getrlimit, setrlimit, Resource};
use tokio::signal;
use tower::service_fn;
use tracing::{debug, instrument};
use tracing_subscriber::prelude::*;

use haproxy::{
    agent::Agent,
    proto::{Action, Capability, Message, MAX_FRAME_SIZE},
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

    /// Enable the support of the specified capability.
    #[arg(short, long, value_enum)]
    capability: Vec<Capability>,

    /// Specify the maximum frame size
    #[arg(short, long, default_value_t = MAX_FRAME_SIZE)]
    max_frame_size: usize,

    /// Set a delay to process a message
    #[arg(short = 't', long)]
    processing_delay: Option<usize>,

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

    let listen = {
        let Opt {
            addr,
            port,
            backlog,
            ..
        } = opt;

        move || {
            net2::TcpBuilder::new_v4()?
                .reuse_address(true)?
                .bind((addr, port))?
                .listen(backlog)
        }
    };

    let listener = if opt.daemonize {
        daemonize(listen, opt.pid_file, opt.chroot)?
    } else {
        listen()?
    };

    serve(listener, opt.max_frame_size)
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

#[tokio::main]
async fn serve(listener: TcpListener, max_frame_size: usize) -> Result<()> {
    rlimit_setnofile()?;

    let agent = Agent::new(listener, max_frame_size)?;
    let shutdown = agent.shutdown();

    tokio::spawn(async move {
        signal::ctrl_c().await.unwrap();

        debug!("received Ctrl+C");

        shutdown.shutdown();
    });

    agent
        .serve(service_fn(|msgs: Vec<Message>| async {
            Ok::<_, Infallible>(vec![])
        }))
        .await?;

    Ok(())
}

fn rlimit_setnofile() -> Result<()> {
    let (sort, hard) = getrlimit(Resource::NOFILE)?;
    setrlimit(Resource::NOFILE, hard, hard)?;

    debug!(from=?sort, to=?hard, "setnofile");

    Ok(())
}
