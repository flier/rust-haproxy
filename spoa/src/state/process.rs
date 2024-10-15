use std::fmt;
use std::sync::Arc;

use derive_more::Debug;
use tokio::time::timeout;
use tower::{MakeService, Service};
use tracing::{info, instrument};

use crate::{
    error::{Context, Result},
    runtime::Runtime,
    spop::{Action, Disconnect, Error::*, Frame, HaproxyNotify, Message, Reassembly},
    state::{AsyncHandler, State},
};

#[derive(Debug)]
pub struct Processing<S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
{
    pub runtime: Arc<Runtime<S, T>>,
    #[debug(skip)]
    pub service: S::Service,
    pub reassembly: Option<Reassembly<Message>>,
}

impl<S, T> Processing<S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
{
    pub fn new(
        runtime: Arc<Runtime<S, T>>,
        service: S::Service,
        reassembly: Option<Reassembly<Message>>,
    ) -> Self {
        Self {
            runtime,
            service,
            reassembly,
        }
    }
}

impl<S, T> AsyncHandler<S, T> for Processing<S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
    S::Error: fmt::Display + Send + Sync + 'static,
{
    #[instrument(skip(self), ret, err, level = "trace")]
    async fn handle_frame(mut self, frame: Frame) -> Result<(State<S, T>, Option<Frame>)> {
        match frame {
            Frame::HaproxyNotify(HaproxyNotify {
                fragmented,
                stream_id,
                frame_id,
                messages,
                ..
            }) => {
                let msgs = if let Some(ref reassembly) = self.reassembly {
                    reassembly.reassemble(fragmented, stream_id, frame_id, messages)?
                } else {
                    Some(messages)
                };

                if let Some(msgs) = msgs {
                    match timeout(self.runtime.max_process_time, self.service.call(msgs)).await {
                        Ok(res) => match res {
                            Ok(actions) => {
                                let ack = Frame::ack(stream_id, frame_id, actions);

                                Ok((self.into(), Some(ack)))
                            }
                            Err(err) => Err(Unknown).context(err.to_string()),
                        },
                        Err(_) => Err(Timeout).context("process messages"),
                    }
                } else {
                    Ok((self.into(), None))
                }
            }
            Frame::HaproxyDisconnect(Disconnect {
                status_code,
                message,
            }) => {
                info!(?status_code, ?message, "disconnecting");

                Err(Normal).context("peer closed connection")
            }
            _ => Err(Invalid).context("unexpected frame"),
        }
    }
}
