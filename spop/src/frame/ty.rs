use num_enum::{IntoPrimitive, TryFromPrimitive};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
pub enum Type {
    Unset,
    /// Sent by HAProxy when it opens a connection on an agent.
    HaproxyHello,
    /// Sent by HAProxy when it want to close the connection or in reply to an AGENT-DISCONNECT frame
    HaproxyDisconnect,
    /// Sent by HAProxy to pass information to an agent
    HaproxyNotify,
    /// Reply to a HAPROXY-HELLO frame, when the connection is established
    AgentHello = 101,
    /// Sent by an agent just before closing the connection
    AgentDisconnect = 102,
    /// Sent to acknowledge a NOTIFY frame
    AgentAck = 103,
}

impl Type {
    pub const UNSET: u8 = Type::Unset as u8;
    pub const HAPROXY_HELLO: u8 = Type::HaproxyHello as u8;
    pub const HAPROXY_DISCON: u8 = Type::HaproxyDisconnect as u8;
    pub const HAPROXY_NOTIFY: u8 = Type::HaproxyNotify as u8;
    pub const AGENT_HELLO: u8 = Type::AgentHello as u8;
    pub const AGENT_DISCON: u8 = Type::AgentDisconnect as u8;
    pub const AGENT_ACK: u8 = Type::AgentAck as u8;
}
