use std::error::Error as StdError;
use std::fmt;
use std::mem;
use std::sync::Arc;

use tokio::{
    io::{AsyncRead, AsyncWrite},
    select,
};
use tokio_util::sync::CancellationToken;
use tower::MakeService;
use tracing::instrument;

use crate::runtime::Runtime;
use crate::{
    error::Result,
    spop::{Action, BufCodec, Codec, Error as Status, Frame, Framer, Message},
    state::AsyncHandler,
    State,
};

#[derive(Debug)]
pub struct Connection<IO, S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
{
    codec: BufCodec<IO>,
    state: State<S, T>,
    tok: CancellationToken,
}

impl<IO, S, T> Connection<IO, S, T>
where
    IO: AsyncRead + AsyncWrite + Unpin,
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
{
    pub fn new(runtime: Arc<Runtime<S, T>>, io: IO, tok: CancellationToken) -> Self {
        let framer = Framer::new(runtime.max_frame_size);
        let codec = Codec::buffered(io, framer);
        let state = State::new(runtime);

        Connection { codec, state, tok }
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

impl<IO, S, T> Connection<IO, S, T>
where
    IO: AsyncRead + AsyncWrite + Unpin,
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
    S::MakeError: StdError + Send + Sync + 'static,
    S::Error: fmt::Display + Send + Sync + 'static,
    T: Clone,
{
    pub async fn serve(&mut self) -> Result<()> {
        loop {
            let state = mem::replace(&mut self.state, State::Disconnecting);
            if matches!(state, State::Disconnecting) {
                break;
            }

            select! {
                _ = self.tok.cancelled() => {
                    break;
                }

                frame = self.codec.read_frame() => {
                    match state.handle_frame(frame?).await {
                        Ok((next, reply)) => {
                            if let Some(frame) = reply {
                                self.codec.write_frame(frame).await?;
                            }
                            self.state = next;
                        }
                        Err(err) => {
                            let frame = Frame::AgentDisconnect(err.into());
                            self.codec.write_frame(frame).await?;
                            self.tok.cancel();
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
