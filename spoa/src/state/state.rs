use std::sync::Arc;

use derive_more::derive::From;

use crate::{
    error::Result,
    runtime::Runtime,
    spop::Frame,
    state::{Handshaking, Processing},
};

pub trait AsyncHandler {
    async fn handle_frame(self, frame: Frame) -> Result<(State, Option<Frame>)>;
}

#[derive(Debug, From)]
pub enum State {
    Initialized(Handshaking),
    Processing(Processing),
    Disconnected,
}

impl State {
    pub fn new(rt: Arc<Runtime>) -> State {
        State::Initialized(Handshaking::new(rt))
    }
}

impl AsyncHandler for State {
    async fn handle_frame(self, frame: Frame) -> Result<(State, Option<Frame>)> {
        match self {
            State::Initialized(handshaking) => handshaking.handle_frame(frame).await,
            State::Processing(processing) => processing.handle_frame(frame).await,
            State::Disconnected => Ok((State::Disconnected, None)),
        }
    }
}
