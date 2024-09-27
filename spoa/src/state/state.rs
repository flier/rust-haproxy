use std::sync::Arc;

use derive_more::derive::From;
use tracing::instrument;

use crate::{
    error::Result,
    runtime::Runtime,
    spop::Frame,
    state::{Connecting, Processing},
};

pub trait AsyncHandler {
    async fn handle_frame(self, frame: Frame) -> Result<(State, Option<Frame>)>;
}

#[derive(Debug, From)]
pub enum State {
    Connecting(Connecting),
    Processing(Processing),
    Disconnecting,
}

impl State {
    pub fn new(rt: Arc<Runtime>) -> State {
        State::Connecting(Connecting::new(rt))
    }
}

impl AsyncHandler for State {
    #[instrument(skip(self), ret, err, level = "trace")]
    async fn handle_frame(self, frame: Frame) -> Result<(State, Option<Frame>)> {
        match self {
            State::Connecting(handshaking) => handshaking.handle_frame(frame).await,
            State::Processing(processing) => processing.handle_frame(frame).await,
            State::Disconnecting => Ok((State::Disconnecting, None)),
        }
    }
}
