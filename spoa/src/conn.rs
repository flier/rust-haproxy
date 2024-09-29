use std::error::Error as StdError;
use std::fmt;
use std::mem;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use tower::Service;
use tracing::instrument;

use crate::runtime::Runtime;
use crate::{
    error::Result,
    spop::{Action, BufCodec, Codec, Error as Status, Frame, Framer, Message},
    state::AsyncHandler,
    State,
};

#[derive(Debug)]
pub struct Connection<IO, S> {
    codec: BufCodec<IO>,
    state: State<S>,
}

impl<IO, S> Connection<IO, S>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(runtime: Arc<Runtime>, io: IO, service: S) -> Self {
        let framer = Framer::new(runtime.max_frame_size);
        let codec = Codec::buffered(io, framer);
        let state = State::new(runtime, service);

        Connection { codec, state }
    }

    #[instrument(skip(self), err, level = "trace")]
    pub async fn disconnect<M>(&mut self, status: Status, msg: M) -> Result<()>
    where
        M: Into<String> + fmt::Debug,
    {
        let disconnect = Frame::agent_disconnect(status, msg);
        self.codec.write_frame(disconnect).await?;
        Ok(())
    }
}

impl<IO, S> Connection<IO, S>
where
    IO: AsyncRead + AsyncWrite + Unpin,
    S: Service<Vec<Message>, Response = Vec<Action>> + Clone + Send + 'static,
    S::Error: StdError,
    S::Future: Send,
{
    pub async fn serve(&mut self) -> Result<()> {
        loop {
            let state = mem::replace(&mut self.state, State::Disconnecting);
            let frame = self.codec.read_frame().await?;

            match state.handle_frame(frame).await {
                Ok((next, reply)) => {
                    if let Some(frame) = reply {
                        self.codec.write_frame(frame).await?;
                    }
                    self.state = next;
                }
                Err(err) => {
                    let frame = Frame::AgentDisconnect(err.into());
                    self.codec.write_frame(frame).await?;
                    break;
                }
            }
        }

        Ok(())
    }
}
