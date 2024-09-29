use std::error::Error as StdError;
use std::net::TcpListener as StdTcpListener;
use std::sync::Arc;

use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
    select,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tower::{MakeService, Service};
use tracing::{debug, instrument, trace};

use crate::{
    error::Result,
    spop::{Action, Error::*, Message},
    Connection, Runtime,
};

#[derive(Debug)]
pub struct Agent<S, T> {
    runtime: Arc<Runtime<S, T>>,
    listener: TcpListener,
    shutdown: Shutdown,
}

impl<S, T> Agent<S, T> {
    pub fn new(runtime: Arc<Runtime<S, T>>, listener: StdTcpListener) -> Result<Self> {
        let listener = TcpListener::from_std(listener)?;

        Ok(Agent {
            runtime,
            listener,
            shutdown: Shutdown::default(),
        })
    }

    pub fn shutdown(&self) -> Shutdown {
        self.shutdown.clone()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Shutdown {
    tracker: TaskTracker,
    token: CancellationToken,
}

impl Shutdown {
    pub fn shutdown(self) {
        self.token.cancel();
    }
}

impl<S, T> Agent<S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>> + Send + Sync + 'static,
    S::Service: Send,
    <S::Service as Service<Vec<Message>>>::Future: Send + 'static,
    S::MakeError: StdError + Send + Sync + 'static,
    S::Future: Send,
    S::Error: StdError + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    pub async fn serve(&self) -> Result<()> {
        loop {
            select! {
                _ = self.shutdown.token.cancelled() => {
                    debug!("shutting down");
                    break
                }
                Ok((stream, peer)) = self.listener.accept() => {
                    trace!(?peer, "accepted connection");

                    let conn = Connection::new(self.runtime.clone(), stream);
                    let tok = self.shutdown.token.child_token();

                    tokio::task::Builder::new()
                        .name("conn")
                        .spawn(self.shutdown.tracker.track_future(async move {
                            process(conn, tok).await
                        }))?;
                }
            }
        }

        self.shutdown.tracker.close();
        self.shutdown.tracker.wait().await;

        Ok(())
    }
}

#[instrument(skip_all, fields(?task = tokio::task::id()), err, level = "trace")]
async fn process<IO, S, T>(mut conn: Connection<IO, S, T>, tok: CancellationToken) -> Result<()>
where
    IO: AsyncRead + AsyncWrite + Unpin,
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
    S::MakeError: StdError + Send + Sync + 'static,
    S::Error: StdError + Send + Sync + 'static,
    T: Clone,
{
    select! {
        _ = tok.cancelled() => {
            conn.disconnect(Normal, "shutting down").await
        }
        res = conn.serve() => {
            res
        }
    }
}
