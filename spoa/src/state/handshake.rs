use std::collections::HashSet;
use std::{cmp, sync::Arc};

use tracing::instrument;

use crate::{
    error::{Context as _, Result},
    proto::MAX_FRAME_SIZE,
    runtime::Runtime,
    spop::{
        AgentHello, Capability,
        Error::{self, NoVersion},
        Frame, HaproxyHello, Version,
    },
    state::{AsyncHandler, Processing, State},
};

#[derive(Debug)]
pub struct Handshaking {
    pub runtime: Arc<Runtime>,
    pub supported_versions: Vec<Version>,
    pub max_frame_size: u32,
    pub capabilities: Vec<Capability>,
}

impl Handshaking {
    pub fn new(runtime: Arc<Runtime>) -> Self {
        Handshaking {
            runtime,
            supported_versions: Version::SUPPORTED.to_vec(),
            max_frame_size: MAX_FRAME_SIZE as u32,
            capabilities: vec![
                Capability::Fragmentation,
                Capability::Async,
                Capability::Pipelining,
            ],
        }
    }
}

impl AsyncHandler for Handshaking {
    async fn handle_frame(self, frame: Frame) -> Result<(State, Option<Frame>)> {
        if let Frame::HaproxyHello(hello) = frame {
            Ok(self.handshake(hello)?)
        } else {
            Err(Error::Invalid).context("expected HaproxyHello frame")
        }
    }
}

impl Handshaking {
    #[instrument(ret, err, level = "trace")]
    fn handshake(self, hello: HaproxyHello) -> Result<(State, Option<Frame>)> {
        let Self {
            runtime,
            supported_versions,
            max_frame_size,
            capabilities,
        } = self;

        let is_healthcheck = hello.healthcheck.unwrap_or_default();
        let handshaked = negotiate(supported_versions, max_frame_size, capabilities, hello)?;
        let frame = handshaked.agent_hello().into();

        if is_healthcheck {
            Ok((State::Disconnected, Some(frame)))
        } else {
            let next = Processing::new(runtime, handshaked);

            Ok((next.into(), Some(frame)))
        }
    }
}

#[instrument(ret, err, level = "trace")]
fn negotiate(
    mut supported_versions: Vec<Version>,
    max_frame_size: u32,
    capabilities: Vec<Capability>,
    mut hello: HaproxyHello,
) -> Result<Negotiated> {
    hello.supported_versions.sort();
    supported_versions.sort();

    let version = hello
        .supported_versions
        .into_iter()
        .rev()
        .find(|version| supported_versions.iter().rev().any(|v| v == version))
        .ok_or(NoVersion)?;
    let max_frame_size = cmp::min(hello.max_frame_size, max_frame_size);
    let capabilities = hello
        .capabilities
        .into_iter()
        .collect::<HashSet<_>>()
        .intersection(&capabilities.into_iter().collect::<HashSet<_>>())
        .cloned()
        .collect::<Vec<_>>();

    Ok(Negotiated {
        version,
        max_frame_size,
        capabilities: capabilities.clone(),
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Negotiated {
    pub version: Version,
    pub max_frame_size: u32,
    pub capabilities: Vec<Capability>,
}

impl Negotiated {
    pub fn agent_hello(&self) -> AgentHello {
        AgentHello {
            version: self.version,
            max_frame_size: self.max_frame_size,
            capabilities: self.capabilities.clone(),
        }
    }
}
