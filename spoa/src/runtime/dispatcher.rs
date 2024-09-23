use dashmap::{DashMap, Entry};
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    oneshot,
};

use crate::{
    error::Result,
    runtime::Acker,
    spop::{AgentAck, FrameId, HaproxyNotify, Message, StreamId},
};

#[derive(Debug, Clone)]
pub struct Dispatcher {
    processing: UnboundedSender<(Acker, UnboundedReceiver<Message>)>,
    receiving: DashMap<(StreamId, FrameId), UnboundedSender<Message>>,
}

impl Dispatcher {
    pub fn new(processing: UnboundedSender<(Acker, UnboundedReceiver<Message>)>) -> Self {
        Self {
            processing,
            receiving: DashMap::new(),
        }
    }

    pub fn recieve_messages(
        &self,
        notify: HaproxyNotify,
    ) -> Result<Option<oneshot::Receiver<AgentAck>>> {
        let key = (notify.stream_id, notify.frame_id);
        let (sender, acked) = {
            match self.receiving.entry(key) {
                Entry::Vacant(e) => {
                    let (sender, receiver) = unbounded_channel();

                    if notify.fragmented {
                        e.insert(sender.clone());
                    }

                    let (acker, acked) = Acker::new(notify.stream_id, notify.frame_id);

                    self.processing.send((acker, receiver))?;

                    (sender, Some(acked))
                }
                Entry::Occupied(e) => {
                    let sender = if notify.fragmented {
                        e.get().clone()
                    } else {
                        e.remove()
                    };

                    (sender, None)
                }
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
