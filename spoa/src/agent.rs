use std::error::Error as StdError;
use std::fmt;
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

    pub fn shutdown(&self) -> CancellationToken {
        self.shutdown.token.clone()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Shutdown {
    tracker: TaskTracker,
    token: CancellationToken,
}

impl<S, T> Agent<S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>> + Send + Sync + 'static,
    S::Service: Send,
    <S::Service as Service<Vec<Message>>>::Future: Send + 'static,
    S::MakeError: StdError + Send + Sync + 'static,
    S::Future: Send,
    S::Error: fmt::Display + Send + Sync + 'static,
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

                    let mut conn = Connection::new(self.runtime.clone(), stream, self.shutdown.token.child_token());

                    tokio::task::Builder::new().name("conn").spawn(self.shutdown.tracker.track_future(async move {
                        conn.serve().await
                    }))?;
                }
            }
        }

        if self.shutdown.tracker.close() && !self.shutdown.tracker.is_empty() {
            debug!(
                conns = self.shutdown.tracker.len(),
                "waiting for shutting down"
            );

            self.shutdown.tracker.wait().await;
        }

        Ok(())
    }
}
