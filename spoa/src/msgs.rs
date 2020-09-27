use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{anyhow, bail, Result};
use tokio::{
    stream::Stream,
    sync::{
        mpsc::{error::TryRecvError::*, UnboundedReceiver},
        oneshot::Sender,
    },
};

use crate::spop::{agent, Action, Data, Message, Scope};

#[derive(Debug)]
pub struct Messages(pub UnboundedReceiver<(Acker, Message)>);

impl Stream for Messages {
    type Item = (Acker, Message);

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.0.try_recv() {
            Ok(res) => Poll::Ready(Some(res)),
            Err(Empty) => Poll::Pending,
            Err(Closed) => Poll::Ready(None),
        }
    }
}

#[derive(Debug)]
pub struct Acker(Option<Inner>);

#[derive(Debug)]
struct Inner(agent::Ack, Sender<agent::Ack>);

impl Drop for Acker {
    fn drop(&mut self) {
        let _ = self.complete();
    }
}

impl Acker {
    pub fn complete(&mut self) -> Result<()> {
        if let Some(Inner(ack, sender)) = self.0.take() {
            sender.send(ack).map_err(|_| anyhow!("receiver closed"))
        } else {
            bail!("already closed")
        }
    }

    pub fn abort(&mut self) -> Result<()> {
        if let Some(Inner(mut ack, sender)) = self.0.take() {
            ack.aborted = true;
            sender.send(ack).map_err(|_| anyhow!("receiver closed"))
        } else {
            bail!("already closed")
        }
    }

    pub fn set_var<S: Into<String>, V: Into<Data>>(&mut self, scope: Scope, name: S, value: V) {
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
