use std::{error::Error as StdError, sync::Arc};

use derive_more::Debug;
use tokio::{sync::oneshot, time::timeout};
use tower::Service;
use tracing::{info, instrument};

use crate::{
    error::{Context, Result},
    runtime::Runtime,
    spop::{Action, AgentAck, Disconnect, Error::*, Frame, HaproxyNotify, Message, Reassembly},
    state::{AsyncHandler, Negotiated, State},
};

#[derive(Debug)]
pub struct Processing<S> {
    pub runtime: Arc<Runtime>,
    #[debug(skip)]
    pub service: S,
    pub handshaked: Negotiated,
    pub reassembly: Reassembly<Message>,
    pub pending: Vec<oneshot::Receiver<AgentAck>>,
}

impl<S> Processing<S> {
    pub fn new(runtime: Arc<Runtime>, service: S, handshaked: Negotiated) -> Self {
        Self {
            runtime,
            service,
            handshaked,
            reassembly: Reassembly::default(),
            pending: vec![],
        }
    }
}

impl<S> AsyncHandler<S> for Processing<S>
where
    S: Service<Vec<Message>, Response = Vec<Action>> + Clone + Send + 'static,
    S::Error: StdError,
    S::Future: Send,
{
    #[instrument(skip(self), ret, err, level = "trace")]
    async fn handle_frame(self, frame: Frame) -> Result<(State<S>, Option<Frame>)> {
        match frame {
            Frame::HaproxyNotify(HaproxyNotify {
                fragmented,
                stream_id,
                frame_id,
                messages,
                ..
            }) => {
                let msgs = if self.handshaked.supports_fragmentation() {
                    self.reassembly
                        .reassemble(fragmented, stream_id, frame_id, messages)?
                } else {
                    Some(messages)
                };

                if let Some(msgs) = msgs {
                    let mut service = self.service.clone();

                    match timeout(self.runtime.max_process_time, service.call(msgs)).await {
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
