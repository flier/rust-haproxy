use std::sync::Arc;

use derive_more::Debug;
use tracing::instrument;

use crate::{
    error::{Context as _, Result},
    runtime::Runtime,
    spop::{Error, Frame, HaproxyHello, Reassembly},
    state::{handshake::negotiate, AsyncHandler, Processing, State},
};

#[derive(Debug)]
pub struct Connecting<S> {
    pub runtime: Arc<Runtime>,
    #[debug(skip)]
    pub service: S,
}

impl<S> Connecting<S> {
    pub fn new(runtime: Arc<Runtime>, service: S) -> Self {
        Connecting { runtime, service }
    }
}

impl<S> AsyncHandler<S> for Connecting<S> {
    async fn handle_frame(self, frame: Frame) -> Result<(State<S>, Option<Frame>)> {
        if let Frame::HaproxyHello(hello) = frame {
            Ok(self.handshake(hello)?)
        } else {
            Err(Error::Invalid).context("expected HaproxyHello frame")
        }
    }
}

impl<S> Connecting<S> {
    #[instrument(skip(self), ret, err, level = "trace")]
    fn handshake(self, hello: HaproxyHello) -> Result<(State<S>, Option<Frame>)> {
        let Self { runtime, service } = self;

        let is_healthcheck = hello.healthcheck.unwrap_or_default();
        let handshaked = {
            negotiate(
                runtime.supported_versions.clone(),
                runtime.max_frame_size,
                runtime.capabilities.clone(),
                hello,
            )?
        };
        let frame = handshaked.agent_hello().into();

        if is_healthcheck {
            Ok((State::Disconnecting, Some(frame)))
        } else {
            let next = Processing::new(
                runtime,
                service,
                handshaked
                    .supports_fragmentation()
                    .then(|| Reassembly::default()),
            );

            Ok((next.into(), Some(frame)))
        }
    }
}
