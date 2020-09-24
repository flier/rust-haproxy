use std::collections::HashMap;

use bitflags::bitflags;

use crate::Data;

pub const SPOE_FRM_T_UNSET: u8 = 0;

/* Frames sent by HAProxy */
pub const SPOE_FRM_T_HAPROXY_HELLO: u8 = 1;
pub const SPOE_FRM_T_HAPROXY_DISCON: u8 = 2;
pub const SPOE_FRM_T_HAPROXY_NOTIFY: u8 = 3;

/* Frames sent by the agents */
pub const SPOE_FRM_T_AGENT_HELLO: u8 = 101;
pub const SPOE_FRM_T_AGENT_DISCON: u8 = 102;
pub const SPOE_FRM_T_AGENT_ACK: u8 = 103;

pub const SPOE_FRM_FL_FIN: u32 = 0x00000001;
pub const SPOE_FRM_FL_ABRT: u32 = 0x00000002;

/* Predefined key used in HELLO/DISCONNECT frames */
pub const SUPPORTED_VERSIONS_KEY: &str = "supported-versions";
pub const VERSION_KEY: &str = "version";
pub const MAX_FRAME_SIZE_KEY: &str = "max-frame-size";
pub const CAPABILITIES_KEY: &str = "capabilities";
pub const ENGINE_ID_KEY: &str = "engine-id";
pub const HEALTHCHECK_KEY: &str = "healthcheck";
pub const STATUS_CODE_KEY: &str = "status-code";
pub const MSG_KEY: &str = "message";

bitflags! {
    /// Flags set on the SPOE frame
    pub struct Flags: u32 {
        const FIN = SPOE_FRM_FL_FIN;
        const ABORT = SPOE_FRM_FL_ABRT;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Metadata {
    pub flags: Flags,
    pub stream_id: u64,
    pub frame_id: u64,
}

/// Frame sent by HAProxy and by agents
#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    /// Used for all frames but the first when a payload is fragmented.
    Unset,
    /// Sent by HAProxy when it opens a connection on an agent.
    HaproxyHello(haproxy::Hello),
    /// Sent by HAProxy when it want to close the connection or in reply to an AGENT-DISCONNECT frame
    HaproxyDisconnect(Disconnect),
    /// Sent by HAProxy to pass information to an agent
    HaproxyNotify(haproxy::Notify),
    /// Reply to a HAPROXY-HELLO frame, when the connection is established
    AgentHello(agent::Hello),
    /// Sent by an agent just before closing the connection
    AgentDisconnect(Disconnect),
    /// Sent to acknowledge a NOTIFY frame
    AgentAck(agent::Ack),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Version {
    pub major: usize,
    pub minor: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Capability {
    /// This is the ability for a peer to support fragmented payload in received frames.
    Fragmentation,
    ///  This is the ability for a peer to decouple NOTIFY and ACK frames.
    Pipelining,
    /// This ability is similar to the pipelining, but here any TCP connection established
    /// between HAProxy and the agent can be used to send ACK frames.
    Async,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub name: String,
    pub args: HashMap<String, Data>,
}

/// If an error occurs, at anytime, from the HAProxy/agent side,
/// a HAPROXY-DISCONNECT/AGENT-DISCONNECT frame is sent with information describing the error.
#[derive(Clone, Debug, PartialEq)]
pub struct Disconnect {
    /// This is the code corresponding to the error.
    pub status_code: u32,
    /// This is the code corresponding to the error.
    pub message: String,
}

pub mod haproxy {
    use crate::{Capability, Message, Version};

    /// This frame is the first one exchanged between HAProxy and an agent, when the connection is established.
    #[derive(Clone, Debug, PartialEq)]
    pub struct Hello {
        /// Last SPOP major versions supported by HAProxy.
        pub supported_versions: Vec<Version>,
        /// This is the maximum size allowed for a frame.
        pub max_frame_size: u32,
        /// This a comma-separated list of capabilities supported by HAProxy.
        pub capabilities: Vec<Capability>,
        /// If this item is set to TRUE, then the HAPROXY-HELLO frame is sent during a SPOE health check.
        pub healthcheck: bool,
        /// This is a uniq string that identify a SPOE engine.
        pub engine_id: Option<String>,
    }

    /// Information are sent to the agents inside NOTIFY frames.
    #[derive(Clone, Debug, PartialEq)]
    pub struct Notify {
        pub fragmented: bool,
        pub stream_id: u64,
        pub frame_id: u64,
        pub messages: Vec<Message>,
    }
}

pub mod agent {
    use crate::{Action, Capability, Version};

    /// This frame is sent in reply to a HAPROXY-HELLO frame to finish a HELLO handshake.
    #[derive(Clone, Debug, PartialEq)]
    pub struct Hello {
        /// This is the SPOP version the agent supports.
        pub version: Version,
        /// This is the maximum size allowed for a frame.
        pub max_frame_size: u32,
        /// This a comma-separated list of capabilities supported by HAProxy.
        pub capabilities: Vec<Capability>,
    }

    /// ACK frames must be sent by agents to reply to NOTIFY frames.
    #[derive(Clone, Debug, PartialEq)]
    pub struct Ack {
        pub fragmented: bool,
        pub stream_id: u64,
        pub frame_id: u64,
        pub actions: Vec<Action>,
    }
}
