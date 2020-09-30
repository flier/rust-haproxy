use std::convert::TryInto;

use anyhow::{Context, Result};
use derive_more::{From, TryInto};
use tokio::sync::oneshot;
use tracing::instrument;

use crate::handshake::{Handshaked, Handshaking};
use crate::msgs::{processing_messages, Dispatcher, Processor};
use crate::spop::{agent, haproxy, Frame, Status};

#[derive(Debug, From, TryInto)]
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
    pub pending_acks: Vec<oneshot::Receiver<agent::Ack>>,
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
                if let Ok(Frame::HaproxyHello(hello)) = frame.try_into() {
                    connecting.handshake(hello)
                } else {
                    Err(Status::Invalid).context("expected HAPROXY-HELLO frame")
                }
            }
            State::Processing(processing) => processing.handle_frame(frame),
        }
    }
}

impl Connecting {
    #[instrument]
    fn handshake(self, hello: haproxy::Hello) -> Result<(State, Option<Frame>)> {
        let healthcheck = hello.healthcheck;
        let handshaked = self.handshaking.handshake(hello)?;

        debug!(?handshaked, "handshaked");

        if healthcheck {
            Err(Status::None).context("healthcheck")
        } else {
            let frame = handshaked.agent_hello().into();
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
            Frame::HaproxyDisconnect(haproxy::Disconnect(disconnect)) => {
                trace!(?disconnect, "peer closed connection");

                Err(Status::None).context("peer closed connection")
            }
            Frame::HaproxyNotify(notify) => {
                if let Some(ack) = self.dispatcher.recieve_messages(notify)? {
                    self.pending_acks.push(ack);
                }

                Ok((self.into(), None))
            }
            _ => {
                warn!(?frame, "unexpected frame");

                Err(Status::Invalid).context("unexpected frame")
            }
        }
    }
}
