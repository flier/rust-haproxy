use std::cmp;
use std::collections::HashSet;
use std::convert::TryInto;

use anyhow::Result;
use derive_more::{From, TryInto};
use tracing::{debug, instrument, trace, warn};

use crate::conn::MAX_FRAME_SIZE;
use crate::spop::{agent, haproxy, Capability, Frame, Status, Version};

#[derive(Debug, From, TryInto)]
pub enum State {
    Connecting(Connecting),
    Processing(Processing),
    Disconnecting,
}

#[derive(Debug)]
pub struct Connecting {
    pub supported_versions: Vec<Version>,
    pub max_frame_size: u32,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug)]
pub struct Processing {
    pub version: Version,
    pub max_frame_size: u32,
    pub capabilities: Vec<Capability>,
}

impl Default for State {
    fn default() -> Self {
        State::Connecting(Connecting {
            supported_versions: vec![Version::default()],
            max_frame_size: MAX_FRAME_SIZE as u32,
            capabilities: vec![
                Capability::Fragmentation,
                Capability::Async,
                Capability::Pipelining,
            ],
        })
    }
}

impl State {
    pub fn is_disconnecting(&self) -> bool {
        match self {
            State::Disconnecting => true,
            _ => false,
        }
    }

    pub fn handle_frame(self, frame: Frame) -> Result<(State, Option<Frame>)> {
        match self {
            State::Connecting(connecting) => {
                if let Ok(Frame::HaproxyHello(hello)) = frame.try_into() {
                    connecting.handshake(hello)
                } else {
                    Ok((
                        State::Disconnecting,
                        Some(Frame::agent_disconnect(
                            Status::None,
                            "expected HAPROXY-HELLO frame",
                        )),
                    ))
                }
            }
            State::Processing(processing) => processing.handle_frame(frame),
            State::Disconnecting => Ok((self, None)),
        }
    }
}

impl Connecting {
    #[instrument(err)]
    fn handshake(mut self, mut hello: haproxy::Hello) -> Result<(State, Option<Frame>)> {
        hello.supported_versions.sort();
        self.supported_versions.sort();

        let version = hello
            .supported_versions
            .into_iter()
            .rev()
            .find(|version| self.supported_versions.iter().any(|v| v == version))
            .ok_or_else(|| Status::NoVersion)?;
        let max_frame_size = cmp::min(hello.max_frame_size, self.max_frame_size);
        let capabilities = hello
            .capabilities
            .into_iter()
            .collect::<HashSet<_>>()
            .intersection(&self.capabilities.into_iter().collect::<HashSet<_>>())
            .cloned()
            .collect::<Vec<_>>();

        debug!(%version, %max_frame_size, capabilities = ?capabilities.as_slice(), "handshaked");

        let next = if hello.healthcheck {
            State::Disconnecting
        } else {
            Processing {
                version,
                max_frame_size,
                capabilities: capabilities.clone(),
            }
            .into()
        };
        let frame = Some(
            agent::Hello {
                version,
                max_frame_size,
                capabilities,
            }
            .into(),
        );

        Ok((next, frame))
    }
}

impl Processing {
    #[instrument(err)]
    fn handle_frame(mut self, frame: Frame) -> Result<(State, Option<Frame>)> {
        match frame {
            Frame::HaproxyDisconnect(haproxy::Disconnect(disconnect)) => {
                trace!(?disconnect, "peer closed connection");

                self.disconnect(Status::None, "peer closed connection")
            }
            _ => {
                warn!(?frame, "unexpected frame");

                self.disconnect(
                    Status::Invalid,
                    format!("unexpected frame: {}", frame.frame_type()),
                )
            }
        }
    }

    fn disconnect<S: Into<String>>(
        mut self,
        status: Status,
        reason: S,
    ) -> Result<(State, Option<Frame>)> {
        Ok((
            State::Disconnecting,
            Some(Frame::agent_disconnect(status, reason).into()),
        ))
    }
}
