use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use derive_more::{From, TryInto};
use tracing::{debug, instrument, trace, warn};

use crate::handshake::{Handshaked, Handshaking};
use crate::spop::{haproxy, Frame, FrameId, Message, Status, StreamId};

#[derive(Clone, Debug, From, TryInto)]
pub enum State {
    Connecting(Connecting),
    Processing(Processing),
}

#[derive(Clone, Debug)]
pub struct Connecting {
    pub handshaking: Handshaking,
}

#[derive(Clone, Debug)]
pub struct Processing {
    pub handshaked: Handshaked,
    pub messages: Arc<Mutex<HashMap<(StreamId, FrameId), Vec<Message>>>>,
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
            let next = Processing {
                handshaked,
                messages: Arc::new(Mutex::new(HashMap::new())),
            }
            .into();

            Ok((next, Some(frame)))
        }
    }
}

impl Processing {
    #[instrument]
    fn handle_frame(self, frame: Frame) -> Result<(State, Option<Frame>)> {
        match frame {
            Frame::HaproxyDisconnect(haproxy::Disconnect(disconnect)) => {
                trace!(?disconnect, "peer closed connection");

                Err(Status::None).context("peer closed connection")
            }
            Frame::HaproxyNotify(haproxy::Notify {
                fragmented,
                stream_id,
                frame_id,
                messages,
            }) => {
                self.messages
                    .lock()
                    .expect("messages")
                    .entry((stream_id, frame_id))
                    .or_insert_with(Vec::new)
                    .extend(messages.into_iter());

                Ok((self.into(), if fragmented { None } else { None }))
            }
            _ => {
                warn!(?frame, "unexpected frame");

                Err(Status::Invalid).context("unexpected frame")
            }
        }
    }
}
