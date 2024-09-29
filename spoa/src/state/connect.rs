use std::error::Error as StdError;
use std::sync::Arc;

use derive_more::Debug;
use tower::MakeService;

use crate::{
    error::{Context as _, Result},
    runtime::Runtime,
    spop::{Action, Error, Frame, HaproxyHello, Message, Reassembly},
    state::{handshake::negotiate, AsyncHandler, Processing, State},
};

#[derive(Debug)]
pub struct Connecting<S, T> {
    pub runtime: Arc<Runtime<S, T>>,
}

impl<S, T> Connecting<S, T> {
    pub fn new(runtime: Arc<Runtime<S, T>>) -> Self {
        Connecting { runtime }
    }
}

impl<S, T> AsyncHandler<S, T> for Connecting<S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
    S::MakeError: StdError + Send + Sync + 'static,
    T: Clone,
{
    async fn handle_frame(self, frame: Frame) -> Result<(State<S, T>, Option<Frame>)> {
        if let Frame::HaproxyHello(hello) = frame {
            self.handshake(hello).await
        } else {
            Err(Error::Invalid).context("expected HaproxyHello frame")
        }
    }
}

impl<S, T> Connecting<S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
    S::MakeError: StdError + Send + Sync + 'static,
    T: Clone,
{
    async fn handshake(self, hello: HaproxyHello) -> Result<(State<S, T>, Option<Frame>)> {
        let Self { runtime } = self;

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
            let service = runtime.service_maker.write().await.make().await?;

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
