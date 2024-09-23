use std::fmt;
use std::mem;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use tracing::instrument;

use crate::runtime::Runtime;
use crate::{
    error::Result,
    proto::MAX_FRAME_SIZE,
    spop::{BufCodec, Codec, Error as Status, Frame, Framer},
    state::AsyncHandler,
    State,
};

#[derive(Debug)]
pub struct Connection<S> {
    codec: BufCodec<S>,
    state: State,
}

impl<S> Connection<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(runtime: Arc<Runtime>, stream: S, max_frame_size: Option<usize>) -> Self {
        let framer = Framer::new(max_frame_size.unwrap_or(MAX_FRAME_SIZE));

        Connection {
            codec: Codec::buffered(stream, framer),
            state: State::new(runtime),
        }
    }

    pub async fn serve(&mut self) -> Result<()> {
        loop {
            let state = mem::replace(&mut self.state, State::Disconnected);
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

    #[instrument(skip(self), ret, err, level = "debug")]
    pub async fn disconnect<M>(&mut self, status: Status, msg: M) -> Result<()>
    where
        M: Into<String> + fmt::Debug,
    {
        let disconnect = Frame::agent_disconnect(status, msg);
        self.codec.write_frame(disconnect).await?;
        Ok(())
    }
}
