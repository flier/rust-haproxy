use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{Context as _, Result};
use futures::ready;
use tokio::net::{TcpListener, ToSocketAddrs};
use tracing::debug;

use crate::{
    msgs::{processing_messages, Dispatcher, Processor},
    spop::{Frame, Status},
    Connection, State,
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

    pub async fn serve(self) -> Serve {
        let (dispatcher, processor) = processing_messages();

        Serve {
            listener: self.listener,
            dispatcher,
            processor,
        }
    }
}

pub struct Serve {
    listener: TcpListener,
    dispatcher: Dispatcher,
    processor: Processor,
}

impl Future for Serve {
    type Output = Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            match ready!(self.as_mut().listener.poll_accept(cx)) {
                Ok((stream, peer)) => {
                    debug!(%peer, "connection accepted");

                    tokio::spawn(process_connection(
                        Connection::new(stream),
                        self.dispatcher.clone(),
                    ));
                }
                Err(err) => return Poll::Ready(Err(err).context("accept failed")),
            }
        }
    }
}

async fn process_connection(mut conn: Connection, dispatcher: Dispatcher) -> Result<()> {
    let mut state = State::default();

    loop {
        let frame = conn.read_frame().await?;
        match state.handle_frame(frame) {
            Ok((next, reply)) => {
                if let Some(frame) = reply {
                    conn.write_frame(frame).await?;
                }
                state = next;
            }
            Err(err) => {
                let reason = err.to_string();
                let status = err.downcast::<Status>().unwrap_or(Status::Unknonw);
                let frame = Frame::agent_disconnect(status, reason);
                conn.write_frame(frame).await?;
                break;
            }
        }
    }

    Ok(())
}
