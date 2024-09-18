//! The frames send by agent.

use crate::{
    data::Value,
    frame::{self, kv, Flags, FrameId, Metadata, StreamId},
    Action, Capability, Version,
};

/// Sent by an agent just before closing the connection.
pub type Disconnect = frame::Disconnect;

/// This frame is sent in reply to a HAPROXY-HELLO frame to finish a HELLO handshake.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    /// This is the SPOP version the agent supports.
    pub version: Version,
    /// This is the maximum size allowed for a frame.
    pub max_frame_size: u32,
    /// This a comma-separated list of capabilities supported by HAProxy.
    pub capabilities: Vec<Capability>,
}

impl Hello {
    pub(crate) fn size(&self) -> usize {
        kv::version(self.version).size()
            + kv::max_frame_size(self.max_frame_size).size()
            + kv::capabilities(&self.capabilities).size()
    }
}

/// ACK frames must be sent by agents to reply to NOTIFY frames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ack {
    pub fragmented: bool,
    pub aborted: bool,
    pub stream_id: StreamId,
    pub frame_id: FrameId,
    pub actions: Vec<Action>,
}

impl Ack {
    pub fn new(stream_id: StreamId, frame_id: FrameId) -> Self {
        Ack {
            fragmented: false,
            aborted: false,
            stream_id,
            frame_id,
            actions: vec![],
        }
    }

    pub fn metadata(&self) -> Metadata {
        Metadata {
            flags: if self.fragmented {
                Flags::empty()
            } else {
                Flags::FIN
            } | if self.aborted {
                Flags::ABORT
            } else {
                Flags::empty()
            },
            stream_id: self.stream_id,
            frame_id: self.frame_id,
        }
    }

    pub(crate) fn size(&self) -> usize {
        self.actions.iter().map(|action| action.size()).sum()
    }
}
