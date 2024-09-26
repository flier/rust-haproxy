use std::fmt;
use std::mem;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use tracing::instrument;

use crate::runtime::Runtime;
use crate::{
    error::Result,
    spop::{BufCodec, Codec, Error as Status, Frame, Framer},
    state::AsyncHandler,
    State,
};

#[derive(Debug)]
pub struct Connection<IO, S> {
    codec: BufCodec<IO>,
    state: State,
    service: S,
}

impl<IO, S> Connection<IO, S>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(runtime: Arc<Runtime>, io: IO, max_frame_size: usize, service: S) -> Self {
        let framer = Framer::new(max_frame_size);
        let codec = Codec::buffered(io, framer);

        Connection {
            codec,
            state: State::new(runtime),
            service,
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
