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
use std::sync::Arc;
use std::{convert::Infallible, fs::File};

use anyhow::{Context, Result};
use clap::Parser;
use daemonize::Daemonize;
use humantime::Duration;
use rlimit::{getrlimit, setrlimit, Resource};
use tokio::signal;
use tower::service_fn;
use tracing::{debug, instrument};
use tracing_subscriber::prelude::*;

use haproxy::{
    agent::{runtime, Agent, Runtime},
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

    /// Specify the number of workers
    #[arg(short, long)]
    num_workers: Option<usize>,

    /// Enable the support of the specified capability.
    #[arg(short, long, value_enum, default_values_t = [Capability::Pipelining])]
    capability: Vec<Capability>,

    /// Specify the maximum frame size
    #[arg(short, long, default_value_t = MAX_FRAME_SIZE)]
    max_frame_size: u32,

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
            .capabilities(opt.capability.iter().copied())
            .max_frame_size(opt.max_frame_size)
            .max_process_time(opt.processing_delay.into())
            .make_service(
                service_fn(|_: ()| async {
                    Ok::<_, Infallible>(service_fn(|msgs: Vec<Message>| async {
                        Ok::<_, Infallible>(vec![])
                    }))
                }),
                (),
            )
    };
    let listener = {
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

        if opt.daemonize {
            daemonize(listen, opt.pid_file, opt.chroot)?
        } else {
            listen()?
        }
    };

    rlimit_setnofile()?;

    let rt = {
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

// #[tokio::main]
// async fn serve<S, T>(runtime: Arc<Runtime<S, T>>, listener: TcpListener) -> Result<()> {
//     let agent = Agent::new(runtime, listener)?;
//     let shutdown = agent.shutdown();

//     tokio::task::Builder::new()
//         .name("singal")
//         .spawn(async move {
//             signal::ctrl_c().await.unwrap();

//             debug!("received Ctrl+C");

//             shutdown.shutdown();
//         })?;

//     agent.serve().await?;

//     Ok(())
// }

fn rlimit_setnofile() -> Result<()> {
    let (sort, hard) = getrlimit(Resource::NOFILE)?;
    setrlimit(Resource::NOFILE, hard, hard)?;

    debug!(from=?sort, to=?hard, "setnofile");

    Ok(())
}
