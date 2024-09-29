use std::{error::Error as StdError, sync::Arc};

use derive_more::{Debug, From};
use tower::MakeService;

use crate::{
    error::Result,
    runtime::Runtime,
    spop::{Action, Frame, Message},
    state::{Connecting, Processing},
};

pub trait AsyncHandler<S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
{
    async fn handle_frame(self, frame: Frame) -> Result<(State<S, T>, Option<Frame>)>;
}

#[derive(Debug, From)]
pub enum State<S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
{
    Connecting(Connecting<S, T>),
    Processing(Processing<S, T>),
    Disconnecting,
}

impl<S, T> State<S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
{
    pub fn new(rt: Arc<Runtime<S, T>>) -> State<S, T> {
        State::Connecting(Connecting::new(rt))
    }
}

impl<S, T> AsyncHandler<S, T> for State<S, T>
where
    S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
    S::MakeError: StdError + Send + Sync + 'static,
    S::Error: StdError + Send + Sync + 'static,
    T: Clone,
{
    async fn handle_frame(self, frame: Frame) -> Result<(State<S, T>, Option<Frame>)> {
        match self {
            State::Connecting(connecting) => connecting.handle_frame(frame).await,
            State::Processing(processing) => processing.handle_frame(frame).await,
            State::Disconnecting => Ok((State::Disconnecting, None)),
        }
    }
}
