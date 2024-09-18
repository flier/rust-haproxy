use derive_more::From;
use tokio::sync::oneshot;
use tracing::instrument;

use crate::{
    error::{Context, Result},
    handshake::{Handshaked, Handshaking},
    msgs::{processing_messages, Dispatcher, Processor},
    spop::{AgentAck, Error, Frame, HaproxyHello},
};

#[derive(Debug, From)]
pub enum State {
    Connecting(Connecting),
    Processing(Processing),
}

#[derive(Debug)]
pub struct Connecting {
    pub handshaking: Handshaking,
}

#[derive(Debug)]
pub struct Processing {
    pub handshaked: Handshaked,
    pub dispatcher: Dispatcher,
    pub processor: Processor,
    pub pending_acks: Vec<oneshot::Receiver<AgentAck>>,
}

impl Default for State {
    fn default() -> Self {
        State::Connecting(Connecting {
            handshaking: Handshaking::default(),
        })
    }
}

impl State {
    pub fn handle_frame(self, frame: Frame) -> Result<(State, Option<Frame>)> {
        match self {
            State::Connecting(connecting) => {
                if let Frame::HaproxyHello(hello) = frame {
                    connecting.handshake(hello)
                } else {
                    Err(Error::Invalid).context("expected HAPROXY-HELLO frame")
                }
            }
            State::Processing(processing) => processing.handle_frame(frame),
        }
    }
}

impl Connecting {
    #[instrument]
    fn handshake(self, hello: HaproxyHello) -> Result<(State, Option<Frame>)> {
        let healthcheck = hello.healthcheck.unwrap_or_default();
        let handshaked = self.handshaking.handshake(hello)?;

        debug!(?handshaked, "handshaked");

        if healthcheck {
            Err(Error::Normal).context("healthcheck")
        } else {
            let frame = Frame::AgentHello(handshaked.agent_hello());
            let (dispatcher, processor) = processing_messages();
            let next = Processing {
                handshaked,
                dispatcher,
                processor,
                pending_acks: vec![],
            }
            .into();

            Ok((next, Some(frame)))
        }
    }
}

impl Processing {
    #[instrument]
    fn handle_frame(mut self, frame: Frame) -> Result<(State, Option<Frame>)> {
        match frame {
            Frame::HaproxyDisconnect(disconnect) => {
                trace!(?disconnect, "peer closed connection");

                Err(Error::Normal).context("peer closed connection")
            }
            Frame::HaproxyNotify(notify) => {
                if let Some(ack) = self.dispatcher.recieve_messages(notify)? {
                    self.pending_acks.push(ack);
                }

                Ok((self.into(), None))
            }
            _ => {
                warn!(?frame, "unexpected frame");

                Err(Error::Invalid).context("unexpected frame")
            }
        }
    }
}
