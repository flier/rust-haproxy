/*
Replicating (mirroring) HTTP requests using the HAProxy SPOP, i.e. Stream
Processing Offload Protocol.

This is a very simple program that can be used to replicate HTTP requests
via the SPOP protocol.  All requests are replicated to the web address (URL)
selected when running the program.
*/
use std::fmt;
use std::io;

use anyhow::Result;
use structopt::StructOpt;
use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    select, signal,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{debug, info, instrument};

use haproxy::{spoa::State, Connection, Error, Frame};

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
    #[structopt(short, long, default_value = "16384")]
    max_frame_size: usize,

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
        _ = serve((opt.address.as_str(), opt.port), token.clone(), tracker.clone()) => {}
        _ = signal::ctrl_c() => {
            tracker.close();
            token.cancel();
        }
    };

    tracker.wait().await;

    Ok(())
}

#[instrument(skip(token, tracker))]
async fn serve<A: ToSocketAddrs + fmt::Debug>(
    addr: A,
    token: CancellationToken,
    tracker: TaskTracker,
) -> Result<()> {
    let listener: TcpListener = TcpListener::bind(addr).await?;

    let tok = token.clone();

    loop {
        select! {
            _ = tok.cancelled() => { break }
            _ = async {
                let (stream, peer) = listener.accept().await?;
                let tok = token.clone();

                info!(?peer, "new connection established");

                tracker.spawn(async move { process(stream, tok).await });

                Ok::<_, io::Error>(())
            }  => {}
        }
    }

    Ok(())
}

#[instrument(skip_all, fields(?task = tokio::task::id(), ?local = stream.local_addr().unwrap(), ?peer = stream.peer_addr().unwrap()), ret, err)]
async fn process(stream: TcpStream, token: CancellationToken) -> Result<()> {
    let mut conn = Connection::new(stream);
    let mut state = State::default();

    loop {
        select! {
            _ = token.cancelled() => {
                let disconnect = Frame::agent_disconnect(Error::Normal, "agent is shutting down");
                conn.write_frame(disconnect).await?;
                break
            }
            res = conn.read_frame() => {
                match res.and_then(|frame| state.handle_frame(frame)) {
                    Ok((next, reply)) => {
                        if let Some(frame) = reply {
                            conn.write_frame(frame).await?;
                        }
                        state = next;
                    }
                    Err(err) => {
                        let reason = err.to_string();
                        let status = err.status().unwrap_or(Error::Unknown);
                        let disconnect = Frame::agent_disconnect(status, reason);
                        conn.write_frame(disconnect).await?;
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
