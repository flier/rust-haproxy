use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{pin_mut, ready};
use pin_project::pin_project;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};

use crate::{
    conn::Connection,
    error::{Context as _, Error, Result},
    runtime::{Acker, Messages, Runtime},
    service::MakeServiceRef,
};

#[derive(Debug)]
pub struct Agent {
    listener: TcpListener,
}

impl Agent {
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> Result<Agent> {
        let listener = TcpListener::bind(addr).await?;

        Ok(Agent { listener })
    }

    pub fn serve<S, IO>(self, new_service: S) -> Serve<S, IO> {
        Serve {
            listener: self.listener,
            new_service,
            runtime: Arc::new(Runtime::default()),
            phantom: PhantomData,
        }
    }
}

#[pin_project]
#[derive(Debug)]
pub struct Serve<S, IO> {
    #[pin]
    listener: TcpListener,
    new_service: S,
    runtime: Arc<Runtime>,
    phantom: PhantomData<IO>,
}

impl<S> Future for Serve<S, TcpStream>
where
    S: MakeServiceRef<Connection<TcpStream>, (Acker, Messages), Error = Error> + Send,
{
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let me = self.project();
        let max_frame_size = me.runtime.max_frame_size;

        loop {
            match ready!(me.listener.poll_accept(cx)) {
                Ok((stream, _)) => {
                    let runtime = me.runtime.clone();
                    let conn = Connection::new(runtime, stream, Some(max_frame_size));

                    tokio::spawn(async move {
                        pin_mut!(conn);

                        conn.serve().await
                    });
                }
                Err(err) => return Poll::Ready(Err(err).context("accept failed")),
            }
        }
    }
}
