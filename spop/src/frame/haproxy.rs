//! The frames send by HAProxy.

use crate::{
    data::Value,
    frame::{self, kv, Flags, FrameId, Message, Metadata, StreamId},
    Capability, Version,
};

/// Sent by HAProxy when it want to close the connection or in reply to an AGENT-DISCONNECT frame.
pub type Disconnect = frame::Disconnect;

/// This frame is the first one exchanged between HAProxy and an agent, when the connection is established.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    /// Last SPOP major versions supported by HAProxy.
    pub supported_versions: Vec<Version>,
    /// This is the maximum size allowed for a frame.
    pub max_frame_size: u32,
    /// This a comma-separated list of capabilities supported by HAProxy.
    pub capabilities: Vec<Capability>,
    /// If this item is set to TRUE, then the HAPROXY-HELLO frame is sent during a SPOE health check.
    pub healthcheck: Option<bool>,
    /// This is a uniq string that identify a SPOE engine.
    pub engine_id: Option<String>,
}

impl Hello {
    pub(crate) fn size(&self) -> usize {
        kv::supported_versions(&self.supported_versions).size()
            + kv::max_frame_size(self.max_frame_size).size()
            + kv::capabilities(&self.capabilities).size()
            + if let Some(healthcheck) = self.healthcheck {
                kv::healthcheck(healthcheck).size()
            } else {
                0
            }
            + if let Some(ref id) = self.engine_id {
                kv::engine_id(id).size()
            } else {
                0
            }
    }
}

/// Information are sent to the agents inside NOTIFY frames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notify {
    /// This is a fragmented frame.
    pub fragmented: bool,
    /// The stream identifier.
    pub stream_id: StreamId,
    /// The frame identifier inside the stream.
    pub frame_id: FrameId,
    /// List of messages.
    pub messages: Vec<Message>,
}

impl Notify {
    /// Returns a metadata representation of this notification
    pub fn metadata(&self) -> Metadata {
        Metadata {
            flags: if self.fragmented {
                Flags::empty()
            } else {
                Flags::FIN
            },
            stream_id: self.stream_id,
            frame_id: self.frame_id,
        }
    }

    /// Returns the size of the frame body in bytes.
    pub(crate) fn size(&self) -> usize {
        self.messages.iter().map(|msg| msg.size()).sum()
    }
}
