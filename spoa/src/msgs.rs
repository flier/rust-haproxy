use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use derive_more::{From, Into};
use futures::Stream;
use tokio::sync::{
    mpsc::{error::TryRecvError::*, unbounded_channel, UnboundedReceiver, UnboundedSender},
    oneshot,
};

use crate::{
    error::{Error::Closed, Result},
    spop::{Action, AgentAck, FrameId, HaproxyNotify, Message, Scope, StreamId, Typed},
};

pub fn processing_messages() -> (Dispatcher, Processor) {
    let (processing, messages) = unbounded_channel();

    (
        Dispatcher {
            processing,
            receiving: HashMap::new(),
        },
        Processor(messages),
    )
}

#[derive(Debug, Clone)]
pub struct Dispatcher {
    processing: UnboundedSender<(Acker, UnboundedReceiver<Message>)>,
    receiving: HashMap<(StreamId, FrameId), UnboundedSender<Message>>,
}

impl Dispatcher {
    pub fn recieve_messages(
        &mut self,
        notify: HaproxyNotify,
    ) -> Result<Option<oneshot::Receiver<AgentAck>>> {
        let key = (notify.stream_id, notify.frame_id);
        let (sender, acked) = {
            if let Entry::Vacant(e) = self.receiving.entry(key) {
                let (sender, receiver) = unbounded_channel();

                if notify.fragmented {
                    e.insert(sender.clone());
                }

                let (acker, acked) = Acker::new(notify.stream_id, notify.frame_id);

                self.processing.send((acker, receiver))?;

                (sender, Some(acked))
            } else {
                let sender = if notify.fragmented {
                    self.receiving.get(&key).cloned()
                } else {
                    self.receiving.remove(&key)
                }
                .expect("sender");

                (sender, None)
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

#[derive(Debug, From, Into)]
pub struct Processor(pub UnboundedReceiver<(Acker, UnboundedReceiver<Message>)>);

impl Stream for Processor {
    type Item = (Acker, Messages);

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.0.try_recv() {
            Ok((acker, receiver)) => Poll::Ready(Some((acker, Messages(receiver)))),
            Err(Empty) => Poll::Pending,
            Err(Disconnected) => Poll::Ready(None),
        }
    }
}

#[derive(Debug, From, Into)]
pub struct Messages(UnboundedReceiver<Message>);

impl Stream for Messages {
    type Item = Message;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.0.try_recv() {
            Ok(res) => Poll::Ready(Some(res)),
            Err(Empty) => Poll::Pending,
            Err(Disconnected) => Poll::Ready(None),
        }
    }
}

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
