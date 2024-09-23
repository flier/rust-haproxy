use std::sync::Arc;

use haproxy_spop::Disconnect;
use tokio::sync::oneshot;
use tracing::{info, instrument};

use crate::{
    error::{Context, Result},
    runtime::Runtime,
    spop::{AgentAck, Error::*, Frame},
    state::{Negotiated, State},
};

use super::AsyncHandler;

#[derive(Debug)]
pub struct Processing {
    pub runtime: Arc<Runtime>,
    pub handshaked: Negotiated,
    pub pending: Vec<oneshot::Receiver<AgentAck>>,
}

impl Processing {
    pub fn new(runtime: Arc<Runtime>, handshaked: Negotiated) -> Self {
        Self {
            runtime,
            handshaked,
            pending: vec![],
        }
    }
}

impl AsyncHandler for Processing {
    #[instrument]
    async fn handle_frame(mut self, frame: Frame) -> Result<(State, Option<Frame>)> {
        match frame {
            Frame::HaproxyDisconnect(Disconnect {
                status_code,
                message,
            }) => {
                info!(?status_code, ?message, "disconnecting");

                Err(Normal).context("peer closed connection")
            }
            Frame::HaproxyNotify(notify) => {
                if let Some(ack) = self.runtime.dispatcher.recieve_messages(notify)? {
                    self.pending.push(ack);
                }

                Ok((self.into(), None))
            }
            _ => Err(Invalid).context("unexpected frame"),
        }
    }
}
