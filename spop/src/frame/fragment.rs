use dashmap::{DashMap, Entry};

use crate::{
    error::{Error, Result},
    frame::{agent::Ack, haproxy::Notify, Frame, FrameId, Message, StreamId},
    Action, AsyncHandler,
};

#[derive(Clone, Debug)]
pub struct Reassembly<T>(Table<T>);

impl AsyncHandler<Option<Vec<Message>>> for Reassembly<Message> {
    type Error = Error;

    async fn handle_frame(&mut self, frame: Frame) -> Result<Option<Vec<Message>>> {
        match frame {
            Frame::HaproxyNotify(Notify {
                fragmented,
                stream_id,
                frame_id,
                messages,
            }) => self
                .0
                .reassemble(fragmented, (stream_id, frame_id), messages),
            Frame::HaproxyDisconnect(_) => Err(Error::Normal),
            _ => Err(Error::Invalid),
        }
    }
}

impl AsyncHandler<Option<Vec<Action>>> for Reassembly<Action> {
    type Error = Error;

    async fn handle_frame(&mut self, frame: Frame) -> Result<Option<Vec<Action>>> {
        match frame {
            Frame::AgentAck(Ack {
                fragmented,
                stream_id,
                frame_id,
                actions,
                ..
            }) => self
                .0
                .reassemble(fragmented, (stream_id, frame_id), actions),
            Frame::AgentDisconnect(_) => Err(Error::Normal),
            _ => Err(Error::Invalid),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Table<T>(DashMap<(StreamId, FrameId), Vec<T>>);

impl<T> Table<T> {
    fn reassemble(
        &self,
        fragmented: bool,
        key: (StreamId, FrameId),
        mut value: Vec<T>,
    ) -> Result<Option<Vec<T>>> {
        match self.0.entry(key) {
            Entry::Occupied(mut e) => {
                if fragmented {
                    e.get_mut().append(&mut value);

                    Ok(None)
                } else {
                    let mut v = e.remove();

                    v.append(&mut value);

                    Ok(Some(v))
                }
            }
            Entry::Vacant(e) => {
                if fragmented {
                    e.insert(value);

                    Ok(None)
                } else {
                    Ok(Some(value))
                }
            }
        }
    }
}
