/*
Replicating (mirroring) HTTP requests using the HAProxy SPOP, i.e. Stream
Processing Offload Protocol.

This is a very simple program that can be used to replicate HTTP requests
via the SPOP protocol.  All requests are replicated to the web address (URL)
selected when running the program.
*/
use std::fmt;
use std::io;
use std::sync::Arc;

use anyhow::Result;
use structopt::StructOpt;
use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    select, signal,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{debug, instrument};

use haproxy::{
    agent::{Connection, Runtime},
    proto::Error::*,
};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "traffic-mirror",
    about = "Replicating (mirroring) HTTP requests using the HAProxy SPOP."
)]
struct Opt {
    /// Specify the address to listen on
    #[structopt(short, long, default_value = "0.0.0.0")]
    address: String,

    /// Specify the port to listen on
    #[structopt(short, long, default_value = "12345")]
    port: u16,

    /// Enable the support of the specified capability.
    #[structopt(short, long)]
    capability: Vec<String>,

    /// Specify the maximum frame size
    #[structopt(short, long)]
    max_frame_size: Option<usize>,

    /// Set a delay to process a message
    #[structopt(short = "t", long)]
    processing_delay: Option<usize>,
}

#[tokio::main]
pub async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let opt = Opt::from_args();

    debug!(?opt);

    let token = CancellationToken::new();
    let tracker = TaskTracker::new();

    select! {
        _ = serve((opt.address.as_str(), opt.port), opt.max_frame_size, token.clone(), tracker.clone()) => {}
        _ = signal::ctrl_c() => {
            token.cancel();
        }
    };

    tracker.close();
    tracker.wait().await;

    Ok(())
}

#[instrument(skip(token, tracker))]
async fn serve<A: ToSocketAddrs + fmt::Debug>(
    addr: A,
    max_frame_size: Option<usize>,
    token: CancellationToken,
    tracker: TaskTracker,
) -> Result<()> {
    let listener: TcpListener = TcpListener::bind(addr).await?;

    let tok = token.clone();

    loop {
        select! {
            _ = tok.cancelled() => { break }
            _ = async {
                let (stream, _) = listener.accept().await?;
                let tok = token.clone();

                tracker.spawn(async move { process(stream,max_frame_size, tok).await });

                Ok::<_, io::Error>(())
            }  => {}
        }
    }

    Ok(())
}

#[instrument(skip_all, fields(?task = tokio::task::id(), ?local = stream.local_addr().unwrap(), ?peer = stream.peer_addr().unwrap()), ret, err, level = "trace")]
async fn process(
    stream: TcpStream,
    max_frame_size: Option<usize>,
    token: CancellationToken,
) -> Result<()> {
    let mut conn = Connection::new(Arc::new(Runtime::default()), stream, max_frame_size);

    loop {
        select! {
            _ = token.cancelled() => {
                conn.disconnect(Normal, "agent is shutting down").await?;

                break
            }
            _ = conn.serve() => {}
        }
    }

    Ok(())
}
