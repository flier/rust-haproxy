use std::sync::Arc;

use tracing::instrument;

use crate::{
    error::{Context as _, Result},
    runtime::Runtime,
    spop::{Capability, Error, Frame, HaproxyHello, Version, MAX_FRAME_SIZE},
    state::{handshake::negotiate, AsyncHandler, Processing, State},
};

#[derive(Debug)]
pub struct Connecting {
    pub runtime: Arc<Runtime>,
    pub supported_versions: Vec<Version>,
    pub max_frame_size: u32,
    pub capabilities: Vec<Capability>,
}

impl Connecting {
    pub fn new(runtime: Arc<Runtime>) -> Self {
        Connecting {
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

impl AsyncHandler for Connecting {
    async fn handle_frame(self, frame: Frame) -> Result<(State, Option<Frame>)> {
        if let Frame::HaproxyHello(hello) = frame {
            Ok(self.handshake(hello)?)
        } else {
            Err(Error::Invalid).context("expected HaproxyHello frame")
        }
    }
}

impl Connecting {
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
            Ok((State::Disconnecting, Some(frame)))
        } else {
            let next = Processing::new(runtime, handshaked);

            Ok((next.into(), Some(frame)))
        }
    }
}
