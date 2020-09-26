use std::cmp;
use std::collections::HashSet;
use std::convert::TryInto;

use anyhow::{Context, Result};
use derive_more::{From, TryInto};
use tracing::{debug, instrument, trace, warn};

use crate::conn::MAX_FRAME_SIZE;
use crate::spop::{agent, haproxy, Capability, Frame, Status, Version};

#[derive(Debug, From, TryInto)]
pub enum State {
    Connecting(Connecting),
    Processing(Processing),
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

        if hello.healthcheck {
            return Err(Status::None).context("healthcheck");
        }

        let next = Processing {
            version,
            max_frame_size,
            capabilities: capabilities.clone(),
        }
        .into();

        let frame = agent::Hello {
            version,
            max_frame_size,
            capabilities,
        }
        .into();

        Ok((next, Some(frame)))
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
            _ => {
                warn!(?frame, "unexpected frame");

                Err(Status::Invalid).context("unexpected frame")
            }
        }
    }
}
