use std::mem;

use derive_more::derive::{From, IsVariant, TryUnwrap};

use crate::{
    frame::{self, Message, Metadata, Type},
    Action, AgentAck, AgentDisconnect, AgentHello, Error, HaproxyDisconnect, HaproxyHello,
    HaproxyNotify,
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

    pub fn notify<I, T>(stream_id: u64, frame_id: u64, msgs: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Message>,
    {
        Frame::HaproxyNotify(HaproxyNotify {
            fragmented: false,
            stream_id,
            frame_id,
            messages: msgs.into_iter().map(|m| m.into()).collect(),
        })
    }

    pub fn ack<I, T>(stream_id: u64, frame_id: u64, actions: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Action>,
    {
        Frame::AgentAck(AgentAck {
            fragmented: false,
            aborted: false,
            stream_id,
            frame_id,
            actions: actions.into_iter().map(|a| a.into()).collect(),
        })
    }

    pub fn haproxy_disconnect<S: Into<String>>(status: Error, reason: S) -> Self {
        Frame::HaproxyDisconnect(frame::Disconnect::new(status, reason))
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

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use bytes::BufMut;

    use crate::{
        data::BufMutExt,
        frame::{agent, decode, encode, haproxy, kv},
        Action, Capability,
        Error::*,
        Scope::{self, *},
        Version,
    };

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
                    encode::action(&mut v, Action::set_var(Scope::Request, "foo", "bar"));
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
                    encode::action(&mut v, Action::unset_var(Scope::Response, "foo"));
                    v
                },
            ),
        ];

        for (a, b) in actions {
            assert_eq!(a.size(), b.len());

            let mut v = Vec::new();
            encode::action(&mut v, a.clone());
            assert_eq!(v, b, "encode::action({a:?}) -> {b:?}");

            assert_eq!(
                decode::action(b.as_slice()),
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
                    supported_versions: vec![Version::V2_0],
                    max_frame_size: 1024,
                    capabilities: vec![Capability::Fragmentation, Capability::Async],
                    healthcheck: None,
                    engine_id: Some("foobar".into()),
                }),
                {
                    let mut v = vec![frame::Type::HAPROXY_HELLO];
                    encode::metadata(&mut v, Metadata::default());
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
                    version: Version::V2_0,
                    max_frame_size: 1024,
                    capabilities: vec![Capability::Fragmentation, Capability::Async],
                }),
                {
                    let mut v = vec![frame::Type::AGENT_HELLO];
                    encode::metadata(&mut v, Metadata::default());
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
                    encode::metadata(
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
                    encode::metadata(
                        &mut v,
                        Metadata {
                            flags: frame::Flags::FIN | frame::Flags::ABORT,
                            stream_id: 123,
                            frame_id: 456,
                        },
                    );

                    encode::action(&mut v, Action::set_var(Scope::Request, "foo", "bar"));
                    encode::action(&mut v, Action::unset_var(Scope::Response, "foo"));

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
                    encode::metadata(&mut v, Metadata::default());
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
                    encode::metadata(&mut v, Metadata::default());
                    v.put_kv(kv::status_code(BadFrameSize as u32));
                    v.put_kv(kv::message("bad frame size"));
                    v
                },
            ),
        ];

        for (f, b) in frames {
            let mut v = Vec::new();
            encode::frame(&mut v, f.clone());
            assert_eq!(&v, &b, "encode frame: {f:?} to {b:?}");
            assert_eq!(
                decode::frame(b.as_slice()),
                Ok(f.clone()),
                "decode frame {f:?} from {b:?}"
            );
        }
    }
}
