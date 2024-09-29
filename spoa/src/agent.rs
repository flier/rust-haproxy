use std::convert::Infallible;
use std::error::Error as StdError;
use std::net::TcpListener as StdTcpListener;
use std::sync::Arc;

use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
    select,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tower::{service_fn, MakeService, Service};
use tracing::{debug, instrument, trace};

use crate::{
    error::Result,
    spop::{Action, Error::*, Message},
    Connection, Runtime,
};

#[derive(Debug)]
pub struct Agent {
    runtime: Arc<Runtime>,
    listener: TcpListener,
    shutdown: Shutdown,
}

impl Agent {
    pub fn new(runtime: Arc<Runtime>, listener: StdTcpListener) -> Result<Agent> {
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

impl Agent {
    pub async fn serve<S>(&self, service: S) -> Result<()>
    where
        S: Service<Vec<Message>, Response = Vec<Action>> + Clone + Send + 'static,
        S::Error: StdError,
        S::Future: Send,
    {
        let new_service = service_fn(|_: ()| async { Ok::<_, Infallible>(service.clone()) });

        self.make_serve(new_service, ()).await
    }

    pub async fn make_serve<S, T>(&self, mut new_service: S, state: T) -> Result<()>
    where
        S: MakeService<T, Vec<Message>, Response = Vec<Action>, MakeError = Infallible>,
        S::Error: StdError,
        S::Service: Service<Vec<Message>, Response = Vec<Action>> + Clone + Send + 'static,
        <S::Service as Service<Vec<Message>>>::Future: Send,
        T: Clone,
    {
        loop {
            select! {
                _ = self.shutdown.token.cancelled() => {
                    debug!("shutting down");
                    break
                }
                Ok((stream, peer)) = self.listener.accept() => {
                    trace!(?peer, "accepted connection");

                    let service = new_service.make_service(state.clone()).await.unwrap();
                    let conn = Connection::new(self.runtime.clone(), stream, service);
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
async fn process<IO, S>(mut conn: Connection<IO, S>, tok: CancellationToken) -> Result<()>
where
    IO: AsyncRead + AsyncWrite + Unpin,
    S: Service<Vec<Message>, Response = Vec<Action>> + Clone + Send + 'static,
    S::Error: StdError,
    S::Future: Send,
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
