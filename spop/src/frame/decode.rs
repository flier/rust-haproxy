use std::iter::{self, FromIterator};
use std::mem;
use std::result::Result as StdResult;
use std::{collections::HashMap, convert::TryFrom};

use bytes::Buf;
use num_enum::TryFromPrimitive;

use crate::{
    action,
    data::BufExt as _,
    error::{Error::*, Result},
    frame::{self, agent, haproxy, kv, Frame, Message, Metadata},
    Action, Capability, Typed, Version,
};

pub trait BufExt {
    fn get_frame(&mut self) -> Result<Frame>;
}

impl<T> BufExt for T
where
    T: Buf,
{
    fn get_frame(&mut self) -> Result<Frame> {
        frame(self)
    }
}

/// Parse a frame from the buffer.
pub fn frame<B: Buf>(mut buf: B) -> Result<Frame> {
    let (ty, md) = frame_type(&mut buf)
        .zip(metadata(&mut buf))
        .ok_or(Invalid)?;

    match ty {
        frame::Type::HaproxyHello if md.stream_id == 0 && md.frame_id == 0 => {
            haproxy_hello(&mut buf).map(Frame::HaproxyHello)
        }
        frame::Type::AgentHello if md.stream_id == 0 && md.frame_id == 0 => {
            agent_hello(&mut buf).map(Frame::AgentHello)
        }
        frame::Type::HaproxyNotify if md.frame_id != 0 => {
            haproxy_notify(&mut buf, md).map(Frame::HaproxyNotify)
        }
        frame::Type::AgentAck if md.frame_id != 0 => agent_ack(&mut buf, md).map(Frame::AgentAck),
        frame::Type::HaproxyDisconnect if md.stream_id == 0 && md.frame_id == 0 => {
            disconnect(&mut buf).map(Frame::HaproxyDisconnect)
        }
        frame::Type::AgentDisconnect if md.stream_id == 0 && md.frame_id == 0 => {
            disconnect(&mut buf).map(Frame::AgentDisconnect)
        }
        _ => Err(Invalid),
    }
}

fn frame_type<B: Buf>(buf: B) -> Option<frame::Type> {
    try_from_u8(buf)
}

fn metadata<B: Buf>(mut buf: B) -> Option<frame::Metadata> {
    let flags = (buf.remaining() >= mem::size_of::<u32>())
        .then(|| buf.get_u32())
        .map(frame::Flags::from_bits_truncate)?;
    let stream_id = buf.varint()?;
    let frame_id = buf.varint()?;

    Some(frame::Metadata {
        flags,
        stream_id,
        frame_id,
    })
}

fn haproxy_hello<B: Buf>(mut buf: B) -> Result<haproxy::Hello> {
    let mut kv = buf.kv_list().collect::<KVList>();

    Ok(haproxy::Hello {
        supported_versions: kv.supported_versions()?,
        max_frame_size: kv.max_frame_size()?,
        capabilities: kv.capabilities()?,
        healthcheck: kv.boolean(kv::HEALTHCHECK_KEY),
        engine_id: kv.string(kv::ENGINE_ID_KEY),
    })
}

fn agent_hello<B: Buf>(mut buf: B) -> Result<agent::Hello> {
    let mut kv = buf.kv_list().collect::<KVList>();

    Ok(agent::Hello {
        version: kv.version()?,
        max_frame_size: kv.max_frame_size()?,
        capabilities: kv.capabilities()?,
    })
}

fn haproxy_notify<B: Buf>(buf: B, md: Metadata) -> Result<haproxy::Notify> {
    Ok(haproxy::Notify {
        fragmented: md.fragmented(),
        stream_id: md.stream_id,
        frame_id: md.frame_id,
        messages: list_of_messages(buf).collect::<Vec<_>>(),
    })
}

fn agent_ack<B: Buf>(buf: B, md: Metadata) -> Result<agent::Ack> {
    Ok(agent::Ack {
        fragmented: md.fragmented(),
        aborted: md.aborted(),
        stream_id: md.stream_id,
        frame_id: md.frame_id,
        actions: list_of_actions(buf).collect::<Vec<_>>(),
    })
}

fn disconnect<B: Buf>(mut buf: B) -> Result<frame::Disconnect> {
    let mut kv = buf.kv_list().collect::<KVList>();

    Ok(haproxy::Disconnect {
        status_code: kv.status_code(),
        message: kv.message(),
    })
}

fn list_of_messages<B: Buf>(mut buf: B) -> impl Iterator<Item = Message> {
    iter::from_fn(move || {
        if buf.has_remaining() {
            message(&mut buf)
        } else {
            None
        }
    })
}

fn message<B: Buf>(mut buf: B) -> Option<Message> {
    let name = buf.string()?;
    let nb = get_u8(&mut buf)?;
    let args = buf.kv_list().take(nb as usize).collect();

    Some(Message { name, args })
}

fn list_of_actions<B: Buf>(mut buf: B) -> impl Iterator<Item = Action> {
    iter::from_fn(move || {
        if buf.has_remaining() {
            action(&mut buf)
        } else {
            None
        }
    })
}

pub fn action<B: Buf>(mut buf: B) -> Option<Action> {
    let ty = action_type(&mut buf)?;
    let nb = get_u8(&mut buf)?;

    match ty {
        action::Type::SetVar if nb == 3 => {
            let scope = scope(&mut buf)?;
            let name = buf.string()?;
            let value = buf.typed()?;

            Some(Action::SetVar { scope, name, value })
        }
        action::Type::UnsetVar if nb == 2 => {
            let scope = scope(&mut buf)?;
            let name = buf.string()?;

            Some(Action::UnsetVar { scope, name })
        }
        _ => None,
    }
}

fn action_type<B: Buf>(buf: B) -> Option<action::Type> {
    try_from_u8(buf)
}

fn scope<B: Buf>(buf: B) -> Option<action::Scope> {
    try_from_u8(buf)
}

struct KVList(HashMap<String, Typed>);

impl FromIterator<(String, Typed)> for KVList {
    fn from_iter<T: IntoIterator<Item = (String, Typed)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl KVList {
    pub fn supported_versions(&mut self) -> Result<Vec<Version>> {
        let s = self.string(kv::SUPPORTED_VERSIONS_KEY).ok_or(NoVersion)?;

        s.split(',')
            .map(|s| s.trim().parse())
            .collect::<StdResult<Vec<_>, _>>()
            .map_err(|_| Invalid)
    }

    pub fn version(&mut self) -> Result<Version> {
        self.string(kv::VERSION_KEY)
            .ok_or(NoVersion)?
            .parse()
            .map_err(|_| BadVersion)
    }

    pub fn max_frame_size(&mut self) -> Result<u32> {
        self.uint(kv::MAX_FRAME_SIZE_KEY)
            .map(|n| n as u32)
            .ok_or(NoFrameSize)
    }

    pub fn capabilities(&mut self) -> Result<Vec<Capability>> {
        let s = self.string(kv::CAPABILITIES_KEY).ok_or(NoCapabilities)?;

        s.split(',')
            .map(|s| s.trim().parse())
            .collect::<StdResult<Vec<_>, _>>()
            .map_err(|_| Invalid)
    }

    pub fn status_code(&mut self) -> u32 {
        self.uint(kv::STATUS_CODE_KEY)
            .map(|n| n as u32)
            .unwrap_or_default()
    }

    pub fn message(&mut self) -> String {
        self.string(kv::MSG_KEY).unwrap_or_default()
    }

    pub fn boolean(&mut self, key: &str) -> Option<bool> {
        self.0.remove(key).and_then(|val| bool::try_from(val).ok())
    }

    pub fn uint(&mut self, key: &str) -> Option<u64> {
        self.0.remove(key).and_then(|val| match val {
            Typed::Int32(n) => Some(n as u64),
            Typed::Uint32(n) => Some(n as u64),
            Typed::Int64(n) => Some(n as u64),
            Typed::Uint64(n) => Some(n),
            _ => None,
        })
    }

    pub fn string(&mut self, key: &str) -> Option<String> {
        self.0
            .remove(key)
            .and_then(|val| String::try_from(val).ok())
    }
}

fn try_from_u8<B: Buf, T: TryFromPrimitive<Primitive = u8>>(buf: B) -> Option<T> {
    let b = get_u8(buf)?;

    T::try_from_primitive(b).ok()
}

fn get_u8<B: Buf>(mut buf: B) -> Option<u8> {
    buf.has_remaining().then(|| buf.get_u8())
}
