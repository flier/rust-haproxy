use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{anyhow, bail, Result};
use tokio::{
    stream::Stream,
    sync::{
        mpsc::{error::TryRecvError::*, unbounded_channel, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
};

use crate::spop::{agent, haproxy, Action, Data, FrameId, Message, Scope, StreamId};

pub fn process_messages() -> (Processor, Messages) {
    let (processing, messages) = unbounded_channel();

    (
        Processor {
            processing,
            receiving: HashMap::new(),
        },
        Messages(messages),
    )
}

pub struct Processor {
    processing: UnboundedSender<(Acker, UnboundedReceiver<Message>)>,
    receiving: HashMap<(StreamId, FrameId), UnboundedSender<Message>>,
}

impl Processor {
    pub async fn recieve_messages(
        &mut self,
        notify: haproxy::Notify,
    ) -> Result<Option<oneshot::Receiver<agent::Ack>>> {
        let key = (notify.stream_id, notify.frame_id);
        let (sender, acked) = {
            if self.receiving.contains_key(&key) {
                let sender = if notify.fragmented {
                    self.receiving.get(&key).cloned()
                } else {
                    self.receiving.remove(&key)
                }
                .expect("sender");

                (sender, None)
            } else {
                let (sender, receiver) = unbounded_channel();

                if notify.fragmented {
                    self.receiving.insert(key, sender.clone());
                }

                let (acker, acked) = Acker::new(notify.stream_id, notify.frame_id);

                self.processing.send((acker, receiver))?;

                (sender, Some(acked))
            }
        };

        for msg in notify.messages {
            if sender.send(msg).is_err() {
                break;
            }
        }

        Ok(acked)
    }
}

#[derive(Debug)]
pub struct Messages(pub UnboundedReceiver<(Acker, UnboundedReceiver<Message>)>);

impl Stream for Messages {
    type Item = (Acker, UnboundedReceiver<Message>);

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
struct Inner(agent::Ack, oneshot::Sender<agent::Ack>);

impl Drop for Acker {
    fn drop(&mut self) {
        let _ = self.complete();
    }
}

impl Acker {
    pub fn new(stream_id: StreamId, frame_id: FrameId) -> (Self, oneshot::Receiver<agent::Ack>) {
        let (sender, receiver) = oneshot::channel();
        (
            Acker(Some(Inner(agent::Ack::new(stream_id, frame_id), sender))),
            receiver,
        )
    }

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
