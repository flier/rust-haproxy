use std::{error::Error as StdError, sync::Arc};

use derive_more::{Debug, From};
use tower::Service;
use tracing::instrument;

use crate::{
    error::Result,
    runtime::Runtime,
    spop::{Action, Frame, Message},
    state::{Connecting, Processing},
};

pub trait AsyncHandler<S> {
    async fn handle_frame(self, frame: Frame) -> Result<(State<S>, Option<Frame>)>;
}

#[derive(Debug, From)]
pub enum State<S> {
    Connecting(Connecting<S>),
    Processing(Processing<S>),
    Disconnecting,
}

impl<S> State<S> {
    pub fn new(rt: Arc<Runtime>, service: S) -> State<S> {
        State::Connecting(Connecting::new(rt, service))
    }
}

impl<S> AsyncHandler<S> for State<S>
where
    S: Service<Vec<Message>, Response = Vec<Action>> + Clone + Send + 'static,
    S::Error: StdError,
    S::Future: Send,
{
    #[instrument(skip(self), ret, err, level = "trace")]
    async fn handle_frame(self, frame: Frame) -> Result<(State<S>, Option<Frame>)> {
        match self {
            State::Connecting(handshaking) => handshaking.handle_frame(frame).await,
            State::Processing(processing) => processing.handle_frame(frame).await,
            State::Disconnecting => Ok((State::Disconnecting, None)),
        }
    }
}
