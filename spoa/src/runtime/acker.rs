use derive_more::Into;
use tokio::sync::oneshot;

use crate::{
    error::{Error::Closed, Result},
    spop::{Action, AgentAck, FrameId, Scope, StreamId, Typed},
};

#[derive(Debug)]
pub struct Acker(Option<Inner>);

#[derive(Debug)]
struct Inner(AgentAck, oneshot::Sender<AgentAck>);

impl Drop for Acker {
    fn drop(&mut self) {
        let _ = self.complete();
    }
}

impl Acker {
    pub fn new(stream_id: StreamId, frame_id: FrameId) -> (Self, oneshot::Receiver<AgentAck>) {
        let (sender, receiver) = oneshot::channel();
        (
            Acker(Some(Inner(AgentAck::new(stream_id, frame_id), sender))),
            receiver,
        )
    }

    pub fn complete(&mut self) -> Result<()> {
        if let Some(Inner(ack, sender)) = self.0.take() {
            sender.send(ack).map_err(|_| Closed)
        } else {
            Err(Closed)
        }
    }

    pub fn abort(&mut self) -> Result<()> {
        if let Some(Inner(mut ack, sender)) = self.0.take() {
            ack.aborted = true;
            sender.send(ack).map_err(|_| Closed)
        } else {
            Err(Closed)
        }
    }

    pub fn set_var<S: Into<String>, V: Into<Typed>>(&mut self, scope: Scope, name: S, value: V) {
        if let Some(Inner(ref mut ack, _)) = self.0 {
            ack.actions.push(Action::SetVar {
                scope,
                name: name.into(),
                value: value.into(),
            });
        }
    }

    pub fn unset_var<S: Into<String>>(&mut self, scope: Scope, name: S) {
        if let Some(Inner(ref mut ack, _)) = self.0 {
            ack.actions.push(Action::UnsetVar {
                scope,
                name: name.into(),
            });
        }
    }
}
