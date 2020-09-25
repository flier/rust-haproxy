use std::mem;
use std::str::FromStr;

use bitflags::bitflags;
use derive_more::Display;

use crate::{
    action::BufMutExt as _,
    data::{BufMutExt as _, Value},
    varint::{self, BufMutExt as _},
    Data,
};

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
    #[derive(Default)]
    pub struct Flags: u32 {
        const FIN = SPOE_FRM_FL_FIN;
        const ABORT = SPOE_FRM_FL_ABRT;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Metadata {
    pub flags: Flags,
    pub stream_id: u64,
    pub frame_id: u64,
}

impl Metadata {
    pub fn size(&self) -> usize {
        mem::size_of::<Flags>() + varint::size_of(self.stream_id) + varint::size_of(self.frame_id)
    }
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

/// SPOP version supported by HAProxy.
#[derive(Clone, Debug, PartialEq, Display)]
#[display(fmt = "{}.{}", major, minor)]
pub struct Version {
    pub major: usize,
    pub minor: usize,
}

impl Version {
    pub fn new(major: usize, minor: usize) -> Self {
        Version { major, minor }
    }
}

/// capabilities supported by HAProxy
#[derive(Clone, Copy, Debug, PartialEq, Display)]
pub enum Capability {
    /// This is the ability for a peer to support fragmented payload in received frames.
    #[display(fmt = "fragmentation")]
    Fragmentation,
    ///  This is the ability for a peer to decouple NOTIFY and ACK frames.
    #[display(fmt = "pipelining")]
    Pipelining,
    /// This ability is similar to the pipelining, but here any TCP connection established
    /// between HAProxy and the agent can be used to send ACK frames.
    #[display(fmt = "async")]
    Async,
}

impl FromStr for Capability {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fragmentation" => Ok(Capability::Fragmentation),
            "pipelining" => Ok(Capability::Pipelining),
            "async" => Ok(Capability::Async),
            _ => Err(s.to_string()),
        }
    }
}

/// define a new SPOE message with the name
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub name: String,
    pub args: Vec<(String, Data)>,
}

impl Message {
    const NB_ARGS_SIZE: usize = mem::size_of::<u8>();

    pub fn size(&self) -> usize {
        self.name.size()
            + Self::NB_ARGS_SIZE
            + self
                .args
                .iter()
                .map(|(k, v)| k.size() + v.size())
                .sum::<usize>()
    }
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

impl Disconnect {
    pub fn size(&self) -> usize {
        (STATUS_CODE_KEY, self.status_code).size() + (MSG_KEY, self.message.as_str()).size()
    }
}

pub mod haproxy {
    use super::*;
    use crate::{
        frame::{Flags, Metadata},
        Capability, Message, Version,
    };

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

    impl Hello {
        pub fn size(&self) -> usize {
            (SUPPORTED_VERSIONS_KEY, self.supported_versions.as_slice()).size()
                + (MAX_FRAME_SIZE_KEY, self.max_frame_size).size()
                + (CAPABILITIES_KEY, self.capabilities.as_slice()).size()
                + if self.healthcheck {
                    (HEALTHCHECK_KEY, self.healthcheck).size()
                } else {
                    0
                }
                + if let Some(ref id) = self.engine_id {
                    (ENGINE_ID_KEY, id.as_str()).size()
                } else {
                    0
                }
        }
    }

    /// Information are sent to the agents inside NOTIFY frames.
    #[derive(Clone, Debug, PartialEq)]
    pub struct Notify {
        pub fragmented: bool,
        pub stream_id: u64,
        pub frame_id: u64,
        pub messages: Vec<Message>,
    }

    impl Notify {
        pub fn metadata(&self) -> Metadata {
            Metadata {
                flags: Flags::default(),
                stream_id: self.stream_id,
                frame_id: self.frame_id,
            }
        }

        pub fn size(&self) -> usize {
            self.messages.iter().map(|msg| msg.size()).sum()
        }
    }
}

pub mod agent {
    use super::*;
    use crate::{
        frame::{Flags, Metadata},
        Action, Capability, Version,
    };

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

    impl Hello {
        pub fn size(&self) -> usize {
            (VERSION_KEY, self.version.to_string()).size()
                + (MAX_FRAME_SIZE_KEY, self.max_frame_size).size()
                + (CAPABILITIES_KEY, self.capabilities.as_slice()).size()
        }
    }

    /// ACK frames must be sent by agents to reply to NOTIFY frames.
    #[derive(Clone, Debug, PartialEq)]
    pub struct Ack {
        pub fragmented: bool,
        pub stream_id: u64,
        pub frame_id: u64,
        pub actions: Vec<Action>,
    }

    impl Ack {
        pub fn metadata(&self) -> Metadata {
            Metadata {
                flags: Flags::default(),
                stream_id: self.stream_id,
                frame_id: self.frame_id,
            }
        }

        pub fn size(&self) -> usize {
            self.actions.iter().map(|action| action.size()).sum()
        }
    }
}

impl Frame {
    const TYPE_SIZE: usize = mem::size_of::<u8>();

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

pub trait BufMutExt {
    fn put_frame(&mut self, frame: Frame);

    fn put_metadata(&mut self, metadata: Metadata);

    fn put_haproxy_hello(&mut self, hello: haproxy::Hello);

    fn put_agent_hello(&mut self, hello: agent::Hello);

    fn put_disconnect(&mut self, disconnect: Disconnect);

    fn put_haproxy_notify(&mut self, notify: haproxy::Notify);

    fn put_agent_ack(&mut self, ack: agent::Ack);
}

impl<T> BufMutExt for T
where
    T: bytes::BufMut,
{
    fn put_frame(&mut self, frame: Frame) {
        match frame {
            Frame::Unset => {
                self.put_u8(SPOE_FRM_T_UNSET);
                self.put_metadata(Metadata::default());
            }

            Frame::HaproxyHello(hello) => {
                self.put_u8(SPOE_FRM_T_HAPROXY_HELLO);
                self.put_metadata(Metadata::default());
                self.put_haproxy_hello(hello);
            }
            Frame::AgentHello(hello) => {
                self.put_u8(SPOE_FRM_T_AGENT_HELLO);
                self.put_metadata(Metadata::default());
                self.put_agent_hello(hello);
            }

            Frame::HaproxyDisconnect(disconnect) => {
                self.put_u8(SPOE_FRM_T_HAPROXY_DISCON);
                self.put_metadata(Metadata::default());
                self.put_disconnect(disconnect);
            }
            Frame::AgentDisconnect(disconnect) => {
                self.put_u8(SPOE_FRM_T_AGENT_DISCON);
                self.put_metadata(Metadata::default());
                self.put_disconnect(disconnect);
            }

            Frame::HaproxyNotify(notify) => {
                self.put_u8(SPOE_FRM_T_HAPROXY_NOTIFY);
                self.put_metadata(notify.metadata());
                self.put_haproxy_notify(notify);
            }
            Frame::AgentAck(ack) => {
                self.put_u8(SPOE_FRM_T_AGENT_ACK);
                self.put_metadata(ack.metadata());
                self.put_agent_ack(ack);
            }
        }
    }

    fn put_metadata(&mut self, metadata: Metadata) {
        self.put_u32(metadata.flags.bits());
        self.put_varint(metadata.stream_id);
        self.put_varint(metadata.frame_id);
    }

    fn put_haproxy_hello(&mut self, hello: haproxy::Hello) {
        self.put_kv(SUPPORTED_VERSIONS_KEY, hello.supported_versions);
        self.put_kv(MAX_FRAME_SIZE_KEY, hello.max_frame_size);
        self.put_kv(CAPABILITIES_KEY, hello.capabilities);
        if hello.healthcheck {
            self.put_kv(HEALTHCHECK_KEY, hello.healthcheck);
        }
        if let Some(ref id) = hello.engine_id {
            self.put_kv(ENGINE_ID_KEY, id.as_str());
        }
    }

    fn put_agent_hello(&mut self, hello: agent::Hello) {
        self.put_kv(VERSION_KEY, hello.version.to_string());
        self.put_kv(MAX_FRAME_SIZE_KEY, hello.max_frame_size);
        self.put_kv(CAPABILITIES_KEY, hello.capabilities);
    }

    fn put_disconnect(&mut self, disconnect: Disconnect) {
        self.put_kv(STATUS_CODE_KEY, disconnect.status_code);
        self.put_kv(MSG_KEY, disconnect.message);
    }

    fn put_haproxy_notify(&mut self, notify: haproxy::Notify) {
        for message in notify.messages {
            self.put_str(message.name);
            self.put_u8(message.args.len() as u8);
            self.put_kvlist(message.args);
        }
    }

    fn put_agent_ack(&mut self, ack: agent::Ack) {
        for action in ack.actions {
            self.put_action(action);
        }
    }
}
