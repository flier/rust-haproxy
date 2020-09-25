/*
Replicating (mirroring) HTTP requests using the HAProxy SPOP, i.e. Stream
Processing Offload Protocol.

This is a very simple program that can be used to replicate HTTP requests
via the SPOP protocol.  All requests are replicated to the web address (URL)
selected when running the program.
*/

use anyhow::{Context, Result};
use structopt::StructOpt;
use tokio::{
    net::{TcpListener, TcpStream},
    stream::StreamExt,
};
use tracing::{debug, instrument, warn};

use haproxy::Connection;

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

impl Opt {}

#[tokio::main]
pub async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let opt = Opt::from_args();
    debug!("opt: {:#?}", opt);

    let mut listener = TcpListener::bind((opt.address.as_str(), opt.port)).await?;

    debug!("listen on: {}", listener.local_addr()?);

    let mut incoming = listener.incoming();

    while let Some(stream) = incoming.next().await {
        match stream {
            Ok(stream) => {
                debug!("client accepted: {}", stream.peer_addr()?);

                tokio::spawn(async move {
                    match process(stream).await {
                        Ok(_) => debug!("client closed"),
                        Err(err) => warn!("client crashed, {}", err),
                    }
                });
            }
            Err(err) => {
                warn!("accept failed: {:?}", err);
                return Err(err.into());
            }
        }
    }

    Ok(())
}

#[instrument]
async fn process(stream: TcpStream) -> Result<()> {
    let mut conn = Connection::new(stream);

    let frame = conn.read_frame().await?;

    Ok(())
}
