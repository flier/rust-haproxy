use std::iter::{self, FromIterator};
use std::mem;
use std::result::Result as StdResult;
use std::{collections::HashMap, convert::TryFrom};

use bytes::{Buf, BufMut};
use num_enum::TryFromPrimitive;

use crate::{
    action,
    data::{BufExt as _, BufMutExt as _},
    error::{Error::*, Result},
    frame::{self, agent, haproxy, kv, Frame, Message, Metadata, Type},
    Action, Capability, Typed, Version,
};

/// Parse a frame from the buffer.
pub fn parse_frame<B: Buf>(mut buf: B) -> Result<Frame> {
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
        frame::Type::HaproxyNotify if md.stream_id != 0 && md.frame_id != 0 => {
            haproxy_notify(&mut buf, md).map(Frame::HaproxyNotify)
        }
        frame::Type::AgentAck if md.stream_id != 0 && md.frame_id != 0 => {
            agent_ack(&mut buf, md).map(Frame::AgentAck)
        }
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
        .then(|| buf.get_u32_ne())
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

fn action<B: Buf>(mut buf: B) -> Option<Action> {
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

/// Put a frame into the buffer.
pub fn put_frame<B: BufMut>(mut buf: B, frame: Frame) {
    match frame {
        Frame::Unset => {
            buf.put_u8(Type::UNSET);
            put_metadata(&mut buf, Metadata::default());
        }

        Frame::HaproxyHello(hello) => {
            buf.put_u8(Type::HAPROXY_HELLO);
            put_metadata(&mut buf, Metadata::default());
            put_haproxy_hello(&mut buf, hello);
        }
        Frame::AgentHello(hello) => {
            buf.put_u8(Type::AGENT_HELLO);
            put_metadata(&mut buf, Metadata::default());
            put_agent_hello(&mut buf, hello);
        }

        Frame::HaproxyDisconnect(disconnect) => {
            buf.put_u8(Type::HAPROXY_DISCON);
            put_metadata(&mut buf, Metadata::default());
            put_disconnect(&mut buf, disconnect);
        }
        Frame::AgentDisconnect(disconnect) => {
            buf.put_u8(Type::AGENT_DISCON);
            put_metadata(&mut buf, Metadata::default());
            put_disconnect(&mut buf, disconnect);
        }

        Frame::HaproxyNotify(notify) => {
            buf.put_u8(Type::HAPROXY_NOTIFY);
            put_metadata(&mut buf, notify.metadata());
            put_haproxy_notify(&mut buf, notify);
        }
        Frame::AgentAck(ack) => {
            buf.put_u8(Type::AGENT_ACK);
            put_metadata(&mut buf, ack.metadata());
            put_agent_ack(&mut buf, ack);
        }
    }
}

fn put_metadata<B: BufMut>(mut buf: B, metadata: Metadata) {
    buf.put_u32(metadata.flags.bits().to_be());
    buf.put_varint(metadata.stream_id);
    buf.put_varint(metadata.frame_id);
}

fn put_haproxy_hello<B: BufMut>(mut buf: B, hello: haproxy::Hello) {
    buf.put_kv(kv::supported_versions(&hello.supported_versions));
    buf.put_kv(kv::max_frame_size(hello.max_frame_size));
    buf.put_kv(kv::capabilities(&hello.capabilities));
    if let Some(healthcheck) = hello.healthcheck {
        buf.put_kv(kv::healthcheck(healthcheck));
    }
    if let Some(ref id) = hello.engine_id {
        buf.put_kv(kv::engine_id(id));
    }
}

fn put_agent_hello<B: BufMut>(mut buf: B, hello: agent::Hello) {
    buf.put_kv(kv::version(hello.version));
    buf.put_kv(kv::max_frame_size(hello.max_frame_size));
    buf.put_kv(kv::capabilities(&hello.capabilities));
}

fn put_disconnect<B: BufMut>(mut buf: B, disconnect: frame::Disconnect) {
    buf.put_kv(kv::status_code(disconnect.status_code));
    buf.put_kv(kv::message(&disconnect.message));
}

fn put_haproxy_notify<B: BufMut>(mut buf: B, notify: haproxy::Notify) {
    for message in notify.messages {
        buf.put_string(message.name);
        buf.put_u8(message.args.len() as u8);
        buf.put_kvlist(message.args);
    }
}

fn put_agent_ack<B: BufMut>(mut buf: B, ack: agent::Ack) {
    for action in ack.actions {
        put_action(&mut buf, action);
    }
}

pub fn put_action<B: BufMut>(mut buf: B, action: Action) {
    match action {
        Action::SetVar { scope, name, value } => {
            buf.put_slice(&[action::Type::SetVar as u8, 3, scope as u8]);
            buf.put_string(name);
            buf.put_typed(value);
        }
        Action::UnsetVar { scope, name } => {
            buf.put_slice(&[action::Type::UnsetVar as u8, 2, scope as u8]);
            buf.put_string(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use crate::Scope::{self, *};

    use super::*;

    #[test]
    fn test_action() {
        let actions = [
            (
                Action::SetVar {
                    scope: Request,
                    name: "foo".into(),
                    value: "bar".into(),
                },
                {
                    let mut v = vec![];
                    put_action(&mut v, Action::set_var(Scope::Request, "foo", "bar"));
                    v
                },
            ),
            (
                Action::UnsetVar {
                    scope: Response,
                    name: "foo".into(),
                },
                {
                    let mut v = vec![];
                    put_action(&mut v, Action::unset_var(Scope::Response, "foo"));
                    v
                },
            ),
        ];

        for (a, b) in actions {
            assert_eq!(a.size(), b.len());

            let mut v = Vec::new();
            put_action(&mut v, a.clone());
            assert_eq!(v, b, "put_action({a:?}) -> {b:?}");

            assert_eq!(
                action(b.as_slice()),
                Some(a.clone()),
                "action({b:?}) -> {a:?}"
            );
        }
    }

    #[test]
    fn test_frame() {
        let frames = [
            (
                Frame::HaproxyHello(haproxy::Hello {
                    supported_versions: vec![Version::new(2, 0)],
                    max_frame_size: 1024,
                    capabilities: vec![Capability::Fragmentation, Capability::Async],
                    healthcheck: None,
                    engine_id: Some("foobar".into()),
                }),
                {
                    let mut v = vec![frame::Type::HAPROXY_HELLO];
                    put_metadata(&mut v, Metadata::default());
                    v.put_kv(kv::supported_versions(&[Version::V2_0]));
                    v.put_kv(kv::max_frame_size(1024));
                    v.put_kv(kv::capabilities(&[
                        Capability::Fragmentation,
                        Capability::Async,
                    ]));
                    v.put_kv(kv::engine_id("foobar"));
                    v
                },
            ),
            (
                Frame::AgentHello(agent::Hello {
                    version: Version::new(2, 0),
                    max_frame_size: 1024,
                    capabilities: vec![Capability::Fragmentation, Capability::Async],
                }),
                {
                    let mut v = vec![frame::Type::AGENT_HELLO];
                    put_metadata(&mut v, Metadata::default());
                    v.put_kv(kv::version(Version::V2_0));
                    v.put_kv(kv::max_frame_size(1024));
                    v.put_kv(kv::capabilities(&[
                        Capability::Fragmentation,
                        Capability::Async,
                    ]));
                    v
                },
            ),
            (
                Frame::HaproxyNotify(haproxy::Notify {
                    fragmented: true,
                    stream_id: 123,
                    frame_id: 456,
                    messages: vec![
                        Message {
                            name: "client".into(),
                            args: vec![
                                ("frontend".into(), "world".into()),
                                ("src".into(), Ipv4Addr::new(127, 0, 0, 1).into()),
                            ],
                        },
                        Message {
                            name: "server".into(),
                            args: vec![
                                ("ip".into(), Ipv6Addr::LOCALHOST.into()),
                                ("port".into(), 80u32.into()),
                            ],
                        },
                    ],
                }),
                {
                    let mut v = vec![frame::Type::HAPROXY_NOTIFY];
                    put_metadata(
                        &mut v,
                        Metadata {
                            flags: frame::Flags::empty(),
                            stream_id: 123,
                            frame_id: 456,
                        },
                    );

                    v.put_string("client");
                    v.put_u8(2);
                    v.put_kv(("frontend", "world"));
                    v.put_kv(("src", Ipv4Addr::new(127, 0, 0, 1)));

                    v.put_string("server");
                    v.put_u8(2);
                    v.put_kv(("ip", Ipv6Addr::LOCALHOST));
                    v.put_kv(("port", 80u32));

                    v
                },
            ),
            (
                Frame::AgentAck(agent::Ack {
                    fragmented: false,
                    aborted: true,
                    stream_id: 123,
                    frame_id: 456,
                    actions: vec![
                        Action::set_var(Scope::Request, "foo", "bar"),
                        Action::unset_var(Scope::Response, "foo"),
                    ],
                }),
                {
                    let mut v = vec![frame::Type::AGENT_ACK];
                    put_metadata(
                        &mut v,
                        Metadata {
                            flags: frame::Flags::FIN | frame::Flags::ABORT,
                            stream_id: 123,
                            frame_id: 456,
                        },
                    );

                    put_action(&mut v, Action::set_var(Scope::Request, "foo", "bar"));
                    put_action(&mut v, Action::unset_var(Scope::Response, "foo"));

                    v
                },
            ),
            (
                Frame::HaproxyDisconnect(frame::Disconnect {
                    status_code: BadVersion as u32,
                    message: "bad version".into(),
                }),
                {
                    let mut v = vec![frame::Type::HAPROXY_DISCON];
                    put_metadata(&mut v, Metadata::default());
                    v.put_kv(kv::status_code(BadVersion as u32));
                    v.put_kv(kv::message("bad version"));
                    v
                },
            ),
            (
                Frame::AgentDisconnect(
                    frame::Disconnect {
                        status_code: BadFrameSize as u32,
                        message: "bad frame size".into(),
                    }
                    .into(),
                ),
                {
                    let mut v = vec![frame::Type::AGENT_DISCON];
                    put_metadata(&mut v, Metadata::default());
                    v.put_kv(kv::status_code(BadFrameSize as u32));
                    v.put_kv(kv::message("bad frame size"));
                    v
                },
            ),
        ];

        for (f, b) in frames {
            let mut v = Vec::new();
            put_frame(&mut v, f.clone());
            assert_eq!(&v, &b, "encode frame: {f:?} to {b:?}");
            assert_eq!(
                parse_frame(b.as_slice()),
                Ok(f.clone()),
                "decode frame {f:?} from {b:?}"
            );
        }
    }
}
