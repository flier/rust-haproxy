use std::mem;

use derive_more::derive::{From, IsVariant, TryUnwrap};

use crate::{
    frame::{self, Metadata, Type},
    AgentAck, AgentDisconnect, AgentHello, Error, HaproxyDisconnect, HaproxyHello, HaproxyNotify,
};

/// Frame sent by HAProxy and by agents
#[derive(Clone, Debug, PartialEq, Eq, From, IsVariant, TryUnwrap)]
pub enum Frame {
    /// Used for all frames but the first when a payload is fragmented.
    #[from(skip)]
    Unset,
    /// Sent by HAProxy when it opens a connection on an agent.
    HaproxyHello(HaproxyHello),
    /// Sent by HAProxy when it want to close the connection or in reply to an AGENT-DISCONNECT frame
    #[from(skip)]
    HaproxyDisconnect(HaproxyDisconnect),
    /// Sent by HAProxy to pass information to an agent
    HaproxyNotify(HaproxyNotify),
    /// Reply to a HAPROXY-HELLO frame, when the connection is established
    AgentHello(AgentHello),
    /// Sent by an agent just before closing the connection
    #[from(skip)]
    AgentDisconnect(AgentDisconnect),
    /// Sent to acknowledge a NOTIFY frame
    AgentAck(AgentAck),
}

impl Frame {
    pub const LENGTH_SIZE: usize = mem::size_of::<u32>();

    pub fn frame_type(&self) -> Type {
        match self {
            Frame::Unset => Type::Unset,
            Frame::HaproxyHello(_) => Type::HaproxyHello,
            Frame::HaproxyDisconnect(_) => Type::HaproxyDisconnect,
            Frame::HaproxyNotify(_) => Type::HaproxyNotify,
            Frame::AgentHello(_) => Type::AgentHello,
            Frame::AgentDisconnect(_) => Type::AgentDisconnect,
            Frame::AgentAck(_) => Type::AgentAck,
        }
    }

    pub fn agent_disconnect<S: Into<String>>(status: Error, reason: S) -> Self {
        Frame::AgentDisconnect(frame::Disconnect::new(status, reason))
    }
}

impl Frame {
    const TYPE_SIZE: usize = mem::size_of::<u8>();

    /// Returns the size of the frame.
    pub fn size(&self) -> usize {
        Self::TYPE_SIZE
            + self.metadata().unwrap_or_default().size()
            + match self {
                Frame::Unset => 0,
                Frame::HaproxyHello(hello) => hello.size(),
                Frame::HaproxyNotify(notify) => notify.size(),
                Frame::AgentHello(hello) => hello.size(),
                Frame::AgentAck(ack) => ack.size(),
                Frame::HaproxyDisconnect(disconnect) | Frame::AgentDisconnect(disconnect) => {
                    disconnect.size()
                }
            }
    }

    pub fn metadata(&self) -> Option<Metadata> {
        match self {
            Frame::HaproxyNotify(notify) => Some(notify.metadata()),
            Frame::AgentAck(ack) => Some(ack.metadata()),
            _ => None,
        }
    }
}
