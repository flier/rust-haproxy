use std::collections::{hash_map::Entry, HashMap};

use crate::{
    error::{Error, Result},
    frame::{agent::Ack, haproxy::Notify, AsyncHandler, Frame, FrameId, Message, StreamId},
    Action,
};

#[derive(Clone, Debug)]
pub struct Reassembly<T>(Table<T>);

impl AsyncHandler for Reassembly<Message> {
    type Output = Option<Vec<Message>>;
    type Error = Error;

    async fn handle_frame(&mut self, frame: Frame) -> Result<Option<Vec<Message>>> {
        if let Frame::HaproxyNotify(Notify {
            fragmented,
            stream_id,
            frame_id,
            messages,
        }) = frame
        {
            self.0
                .reassemble(fragmented, (stream_id, frame_id), messages)
        } else {
            Ok(None)
        }
    }
}

impl AsyncHandler for Reassembly<Action> {
    type Output = Option<Vec<Action>>;
    type Error = Error;

    async fn handle_frame(&mut self, frame: Frame) -> Result<Option<Vec<Action>>> {
        if let Frame::AgentAck(Ack {
            fragmented,
            stream_id,
            frame_id,
            actions,
            ..
        }) = frame
        {
            self.0
                .reassemble(fragmented, (stream_id, frame_id), actions)
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, Debug)]
pub struct Table<T>(HashMap<(StreamId, FrameId), Vec<T>>);

impl<T> Table<T> {
    fn reassemble(
        &mut self,
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
