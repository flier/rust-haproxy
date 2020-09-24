use bitflags::bitflags;

const SPOE_FRM_T_UNSET: u8 = 0;

/* Frames sent by HAProxy */
const SPOE_FRM_T_HAPROXY_HELLO: u8 = 1;
const SPOE_FRM_T_HAPROXY_DISCON: u8 = 2;
const SPOE_FRM_T_HAPROXY_NOTIFY: u8 = 3;

/* Frames sent by the agents */
const SPOE_FRM_T_AGENT_HELLO: u8 = 101;
const SPOE_FRM_T_AGENT_DISCON: u8 = 102;
const SPOE_FRM_T_AGENT_ACK: u8 = 103;

/// Frame Types sent by HAProxy and by agents
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Type {
    /// Used for all frames but the first when a payload is fragmented.
    Unset = SPOE_FRM_T_UNSET,
    /// Sent by HAProxy when it opens a connection on an agent.
    HaproxyHello = SPOE_FRM_T_HAPROXY_HELLO,
    /// Sent by HAProxy when it want to close the connection or in reply to an AGENT-DISCONNECT frame
    HaproxyDisconnect = SPOE_FRM_T_HAPROXY_DISCON,
    /// Sent by HAProxy to pass information to an agent
    HaproxyNotify = SPOE_FRM_T_HAPROXY_NOTIFY,
    /// Reply to a HAPROXY-HELLO frame, when the connection is established
    AgentHello = SPOE_FRM_T_AGENT_HELLO,
    /// Sent by an agent just before closing the connection
    AgentDisconnect = SPOE_FRM_T_AGENT_DISCON,
    /// Sent to acknowledge a NOTIFY frame
    AgentAck = SPOE_FRM_T_AGENT_ACK,
}

const SPOE_FRM_FL_FIN: u32 = 0x00000001;
const SPOE_FRM_FL_ABRT: u32 = 0x00000002;

bitflags! {
    /// Flags set on the SPOE frame
    pub struct Flags: u32 {
        const FIN = SPOE_FRM_FL_FIN;
        const ABORT = SPOE_FRM_FL_ABRT;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub ty: Type,
    pub flags: Flags,
    pub stream_id: u64,
    pub frame_id: u64,
}
