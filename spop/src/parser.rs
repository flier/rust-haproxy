use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::iter::FromIterator;
use std::net::{Ipv4Addr, Ipv6Addr};

use bytes::Bytes;
use combine::{
    any, count_min_max,
    error::{ParseError, StreamError},
    from_str, many1,
    parser::{
        byte::{byte, num::be_u32},
        char::{self, char, digit, spaces},
        choice::choice,
        range::{take, take_fn},
    },
    sep_by1,
    stream::{easy, position, Range, Stream, StreamErrorFor},
    struct_parser, token, value, EasyParser, Parser, RangeStreamOnce,
};

use crate::{action::*, data::*, frame::*, varint::BufExt, Status::*};

type PositionStream<'a> = position::Stream<&'a [u8], position::IndexPositioner>;

/*
Exchange between HAProxy and agents are made using FRAME packets. All frames
must be prefixed with their size encoded on 4 bytes in network byte order:

    <FRAME-LENGTH:4 bytes> <FRAME>

A frame always starts with its type, on one byte, followed by metadata
containing flags, on 4 bytes and a two variable-length integer representing the
stream identifier and the frame identifier inside the stream:

    FRAME       : <FRAME-TYPE:1 byte> <METADATA> <FRAME-PAYLOAD>
    METADATA    : <FLAGS:4 bytes> <STREAM-ID:varint> <FRAME-ID:varint>

Then comes the frame payload. Depending on the frame type, the payload can be
of three types: a simple key/value list, a list of messages or a list of
actions.

    FRAME-PAYLOAD    : <LIST-OF-MESSAGES> | <LIST-OF-ACTIONS> | <KV-LIST>

    LIST-OF-MESSAGES : [ <MESSAGE-NAME> <NB-ARGS:1 byte> <KV-LIST> ... ]
    MESSAGE-NAME     : <STRING>

    LIST-OF-ACTIONS  : [ <ACTION-TYPE:1 byte> <NB-ARGS:1 byte> <ACTION-ARGS> ... ]
    ACTION-ARGS      : [ <TYPED-DATA>... ]

    KV-LIST          : [ <KV-NAME> <KV-VALUE> ... ]
    KV-NAME          : <STRING>
    KV-VALUE         : <TYPED-DATA>

    FLAGS :

    Flags are a 32 bits field. They are encoded on 4 bytes in network byte
    order, where the bit 0 is the LSB.

              0   1      2-31
            +---+---+----------+
            |   | A |          |
            | F | B |          |
            | I | O | RESERVED |
            | N | R |          |
            |   | T |          |
            +---+---+----------+

    FIN: Indicates that this is the final payload fragment. The first fragment
         may also be the final fragment.

    ABORT: Indicates that the processing of the current frame must be
           cancelled. This bit should be set on frames with a fragmented
           payload. It can be ignore for frames with an unfragemnted
           payload. When it is set, the FIN bit must also be set.
*/

impl Frame {
    pub fn parse(b: &[u8]) -> Result<(Frame, PositionStream), easy::ParseError<PositionStream>> {
        frame().easy_parse(position::Stream::new(b))
    }
}

fn frame<Input>() -> impl Parser<Input, Output = Frame>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    let fragment_not_supported = |metadata: Metadata| {
        if metadata.fragmented() {
            Err(StreamErrorFor::<Input>::other(FragmentNotSupported))
        } else {
            Ok(metadata)
        }
    };

    choice((
        (token(SPOE_FRM_T_UNSET), metadata()).map(|_| Frame::Unset),
        (
            token(SPOE_FRM_T_HAPROXY_HELLO),
            metadata().and_then(fragment_not_supported),
        )
            .with(haproxy_hello())
            .map(Frame::HaproxyHello)
            .expected("haproxy::hello"),
        (
            token(SPOE_FRM_T_HAPROXY_DISCON),
            metadata().and_then(fragment_not_supported),
        )
            .with(disconnect())
            .map(haproxy::Disconnect)
            .map(Frame::HaproxyDisconnect)
            .expected("haproxy::disconnect"),
        token(SPOE_FRM_T_HAPROXY_NOTIFY)
            .with(metadata())
            .then(haproxy_notify)
            .map(Frame::HaproxyNotify)
            .expected("haproxy::notify"),
        (
            token(SPOE_FRM_T_AGENT_HELLO),
            metadata().and_then(fragment_not_supported),
        )
            .with(agent_hello())
            .map(Frame::AgentHello)
            .expected("agent::hello"),
        (
            token(SPOE_FRM_T_AGENT_DISCON),
            metadata().and_then(fragment_not_supported),
        )
            .with(disconnect())
            .map(agent::Disconnect)
            .map(Frame::AgentDisconnect)
            .expected("agent::disconnect"),
        token(SPOE_FRM_T_AGENT_ACK)
            .with(metadata())
            .then(agent_ack)
            .map(Frame::AgentAck)
            .expected("agent::ack"),
    ))
    .expected("frame")
}

fn metadata<Input>() -> impl Parser<Input, Output = Metadata>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    (struct_parser! {
        Metadata {
            flags: be_u32().map(Flags::from_bits_truncate).expected("flags"),
            stream_id: varint().expected("stream_id"),
            frame_id: varint().expected("frame_id"),
        }
    })
    .expected("metadata")
}

#[derive(Clone, Debug, PartialEq)]
pub struct KVList(pub HashMap<String, Data>);

impl<K> FromIterator<(K, Data)> for KVList
where
    K: Into<String>,
{
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (K, Data)>,
    {
        KVList(iter.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

fn kvlist<Input>() -> impl Parser<Input, Output = KVList>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    many1((string().expected("key"), data().expected("value")))
        .map(KVList)
        .expected("kvlist")
}

impl KVList {
    pub fn extract<T>(&mut self, key: &'static str) -> Option<T>
    where
        T: TryFrom<Data>,
    {
        self.0.remove(key).and_then(|value| value.try_into().ok())
    }

    pub fn extract_into<T, E, Item, Range>(&mut self, key: &'static str) -> Result<T, E>
    where
        T: TryFrom<Data, Error = &'static str>,
        E: StreamError<Item, Range> + Sized,
    {
        self.0
            .remove(key)
            .ok_or_else(|| E::expected_static_message(key))?
            .try_into()
            .map_err(E::expected_static_message)
    }
}

fn haproxy_hello<Input>() -> impl Parser<Input, Output = haproxy::Hello>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    kvlist().and_then::<_, _, StreamErrorFor<Input>>(|mut kvs| {
        Ok(haproxy::Hello {
            supported_versions: kvs
                .extract::<String>(SUPPORTED_VERSIONS_KEY)
                .ok_or_else(|| StreamErrorFor::<Input>::other(NoVersion))
                .and_then(|s| {
                    versions()
                        .easy_parse(position::Stream::new(s.as_str()))
                        .map(|versions| versions.0)
                        .map_err(|_| StreamErrorFor::<Input>::other(BadVersion))
                })?,
            max_frame_size: kvs
                .extract(MAX_FRAME_SIZE_KEY)
                .ok_or_else(|| StreamErrorFor::<Input>::other(NoFrameSize))?,
            capabilities: kvs
                .extract::<String>(CAPABILITIES_KEY)
                .ok_or_else(|| StreamErrorFor::<Input>::other(NoCapabilities))
                .and_then(|s| {
                    capabilities()
                        .easy_parse(position::Stream::new(s.as_str()))
                        .map(|r| r.0)
                        .map_err(|_| StreamErrorFor::<Input>::other(BadVersion))
                })?,
            healthcheck: kvs.extract(HEALTHCHECK_KEY).unwrap_or_default(),
            engine_id: kvs.extract(ENGINE_ID_KEY),
        })
    })
}

fn agent_hello<Input>() -> impl Parser<Input, Output = agent::Hello>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    kvlist().and_then::<_, _, StreamErrorFor<Input>>(|mut kvs| {
        Ok(agent::Hello {
            version: kvs
                .extract::<String>(VERSION_KEY)
                .ok_or_else(|| StreamErrorFor::<Input>::other(NoVersion))
                .and_then(|s| {
                    version()
                        .easy_parse(position::Stream::new(s.as_str()))
                        .map(|r| r.0)
                        .map_err(|_| StreamErrorFor::<Input>::other(BadVersion))
                })?,
            max_frame_size: kvs
                .extract(MAX_FRAME_SIZE_KEY)
                .ok_or_else(|| StreamErrorFor::<Input>::other(NoFrameSize))?,
            capabilities: kvs
                .extract::<String>(CAPABILITIES_KEY)
                .ok_or_else(|| StreamErrorFor::<Input>::other(NoCapabilities))
                .and_then(|s| {
                    capabilities()
                        .easy_parse(position::Stream::new(s.as_str()))
                        .map(|r| r.0)
                        .map_err(|_| StreamErrorFor::<Input>::other(BadVersion))
                })?,
        })
    })
}

fn versions<Input>() -> impl Parser<Input, Output = Vec<Version>>
where
    Input: Stream<Token = char>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    sep_by1(version(), (spaces(), token(','), spaces())).expected("versions")
}

fn version<Input>() -> impl Parser<Input, Output = Version>
where
    Input: Stream<Token = char>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    (struct_parser! {
        Version {
            major: from_str(many1::<String, _, _>(digit())).expected("major"),
            _: char('.'),
            minor: from_str(many1::<String, _, _>(digit())).expected("minor"),
        }
    })
    .expected("version")
}

fn capabilities<Input>() -> impl Parser<Input, Output = Vec<Capability>>
where
    Input: Stream<Token = char>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    sep_by1(
        from_str::<Input, Capability, _>(many1::<String, _, _>(char::letter())),
        (spaces(), token(','), spaces()),
    )
    .expected("capabilities")
}

fn haproxy_notify<Input>(metadata: Metadata) -> impl Parser<Input, Output = haproxy::Notify>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    use crate::frame::haproxy::Notify;

    struct_parser! {
        Notify {
            fragmented: value(metadata.fragmented()),
            stream_id: value(metadata.stream_id),
            frame_id: value(metadata.frame_id),
            messages: many1::<Vec<_>, _, _>(message().expected("message")).expected("messages"),
        }
    }
}

fn message<Input>() -> impl Parser<Input, Output = Message>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    struct_parser! {
        Message {
            name: string(),
            args: any().then(|nb| {
                count_min_max::<Vec<_>, _, _>(nb as usize, nb as usize, (string().expected("key"), data().expected("value")))
            }).expected("args"),
        }
    }
}

fn agent_ack<Input>(metadata: Metadata) -> impl Parser<Input, Output = agent::Ack>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    use crate::frame::agent::Ack;

    struct_parser! {
        Ack {
            fragmented: value(metadata.fragmented()),
            stream_id: value(metadata.stream_id),
            frame_id: value(metadata.frame_id),
            actions: many1::<Vec<_>, _, _>(action()).expected("actions"),
        }
    }
}

fn action<Input>() -> impl Parser<Input, Output = Action>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    use crate::Action::*;

    choice((
        struct_parser! {
            SetVar {
                _: token(SPOE_ACT_T_SET_VAR),
                _: byte(3), // SET-VAR requires 3 arguments
                scope: scope(),
                name: string().expected("name"),
                value: data().expected("value"),
            }
        },
        struct_parser! {
            UnsetVar {
                _: token(SPOE_ACT_T_UNSET_VAR),
                _: byte(2), // UNSET-VAR requires 2 arguments
                scope: scope(),
                name: string().expected("name"),
            }
        },
    ))
    .expected("action")
}

fn scope<Input>() -> impl Parser<Input, Output = Scope>
where
    Input: Stream<Token = u8>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    choice((
        token(SPOE_SCOPE_PROC).map(|_| Scope::Process),
        token(SPOE_SCOPE_SESS).map(|_| Scope::Session),
        token(SPOE_SCOPE_TXN).map(|_| Scope::Transaction),
        token(SPOE_SCOPE_REQ).map(|_| Scope::Request),
        token(SPOE_SCOPE_RES).map(|_| Scope::Response),
    ))
    .expected("scope")
}

fn disconnect<Input>() -> impl Parser<Input, Output = Disconnect>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    kvlist().and_then::<_, _, StreamErrorFor<Input>>(|mut kvs| {
        Ok(Disconnect {
            status_code: kvs.extract_into(STATUS_CODE_KEY)?,
            message: kvs.extract_into(MSG_KEY)?,
        })
    })
}

/*
3.1. Data types
----------------

Here is the bytewise representation of typed data:

    TYPED-DATA    : <TYPE:4 bits><FLAGS:4 bits><DATA>

Supported types and their representation are:

    TYPE                       |  ID | DESCRIPTION
  -----------------------------+-----+----------------------------------
     NULL                      |  0  |  NULL   : <0>
     Boolean                   |  1  |  BOOL   : <1+FLAG>
     32bits signed integer     |  2  |  INT32  : <2><VALUE:varint>
     32bits unsigned integer   |  3  |  UINT32 : <3><VALUE:varint>
     64bits signed integer     |  4  |  INT64  : <4><VALUE:varint>
     32bits unsigned integer   |  5  |  UNIT64 : <5><VALUE:varint>
     IPV4                      |  6  |  IPV4   : <6><STRUCT IN_ADDR:4 bytes>
     IPV6                      |  7  |  IPV6   : <7><STRUCT IN_ADDR6:16 bytes>
     String                    |  8  |  STRING : <8><LENGTH:varint><BYTES>
     Binary                    |  9  |  BINARY : <9><LENGTH:varint><BYTES>
    10 -> 15  unused/reserved  |  -  |  -
  -----------------------------+-----+----------------------------------
*/

fn data<Input>() -> impl Parser<Input, Output = Data>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    choice((
        token(SPOE_DATA_T_NULL).map(|_| Data::Null),
        token(SPOE_DATA_T_BOOL | SPOE_DATA_FL_FALSE).map(|_| Data::Boolean(false)),
        token(SPOE_DATA_T_BOOL | SPOE_DATA_FL_TRUE).map(|_| Data::Boolean(true)),
        token(SPOE_DATA_T_INT32)
            .with(varint())
            .map(|n| Data::Int32(n as i32))
            .expected("int32"),
        token(SPOE_DATA_T_UINT32)
            .with(varint())
            .map(|n| Data::Uint32(n as u32))
            .expected("uint32"),
        token(SPOE_DATA_T_INT64)
            .with(varint())
            .map(|n| Data::Int64(n as i64))
            .expected("int64"),
        token(SPOE_DATA_T_UINT64)
            .with(varint())
            .map(Data::Uint64)
            .expected("uint64"),
        token(SPOE_DATA_T_IPV4)
            .with(take(Data::IPV4_ADDR_LEN))
            .and_then(|b: Input::Range| {
                <[u8; Data::IPV4_ADDR_LEN]>::try_from(b.as_ref())
                    .map(Ipv4Addr::from)
                    .map(Data::IPv4)
                    .map_err(StreamErrorFor::<Input>::other)
            })
            .expected("ipv4"),
        token(SPOE_DATA_T_IPV6)
            .with(take(Data::IPV6_ADDR_LEN))
            .and_then(|b: Input::Range| {
                <[u8; Data::IPV6_ADDR_LEN]>::try_from(b.as_ref())
                    .map(Ipv6Addr::from)
                    .map(Data::IPv6)
                    .map_err(StreamErrorFor::<Input>::other)
            })
            .expected("ipv6"),
        token(SPOE_DATA_T_STR).with(string()).map(Data::String),
        token(SPOE_DATA_T_BIN).with(binary()).map(Data::Binary),
    ))
}

fn string<Input>() -> impl Parser<Input, Output = String>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    varint()
        .then(|n| take(n as usize))
        .and_then(|b: Input::Range| {
            String::from_utf8(b.as_ref().to_vec()).map_err(StreamErrorFor::<Input>::other)
        })
        .expected("string")
}

fn binary<Input>() -> impl Parser<Input, Output = Bytes>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    varint()
        .then(|n| take(n as usize))
        .map(|b: Input::Range| Bytes::copy_from_slice(b.as_ref()))
        .expected("binary")
}

fn varint<Input>() -> impl Parser<Input, Output = u64>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    take_fn(|b: Input::Range| b.as_ref().iter().position(|&b| b < 0x80).map(|n| n + 1))
        .map(|b: Input::Range| b.as_ref().get_varint())
        .expected("varint")
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use bytes::BufMut;
    use combine::stream::position::Stream;
    use lazy_static::lazy_static;

    use super::*;
    use crate::{
        data::BufMutExt as _,
        frame::{agent, haproxy, BufMutExt as _},
        Data::*,
        Status,
    };

    lazy_static! {
        static ref TEST_DATA: Vec<(Data, &'static [u8])> = [
            (Null, &[SPOE_DATA_T_NULL][..]),
            (Boolean(true), &[SPOE_DATA_T_BOOL | SPOE_DATA_FL_TRUE][..]),
            (Boolean(false), &[SPOE_DATA_T_BOOL | SPOE_DATA_FL_FALSE][..]),
            (Int32(123), &[SPOE_DATA_T_INT32, 123][..]),
            (Uint32(456), &[SPOE_DATA_T_UINT32, 0xf8, 0x0d][..]),
            (Int64(789), &[SPOE_DATA_T_INT64, 0xf5, 0x22][..]),
            (Uint64(999), &[SPOE_DATA_T_UINT64, 0xf7, 0x2f][..]),
            (
                IPv4(Ipv4Addr::new(127, 0, 0, 1)),
                &[SPOE_DATA_T_IPV4, 127, 0, 0, 1],
            ),
            (
                IPv6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc00a, 0x2ff)),
                &[
                    SPOE_DATA_T_IPV6,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0xff,
                    0xff,
                    0xc0,
                    0x0a,
                    0x02,
                    0xff,
                ],
            ),
            (String("hello world".to_string()), b"\x08\x0bhello world"),
            (
                Binary(Bytes::from_static(b"hello world")),
                b"\x09\x0bhello world"
            ),
        ]
        .to_vec();
    }

    #[test]
    fn test_data() {
        for (d, b) in TEST_DATA.iter() {
            assert_eq!(d.size(), b.len(), "data: {:?}", d);

            let mut v = Vec::new();
            v.put_data(d.clone());
            assert_eq!(v.as_slice(), *b, "encode data: {:?}", d);

            let (r, s) = data().easy_parse(Stream::new(*b)).unwrap();
            assert_eq!(r, d.clone(), "decode data: {:?}", b);
            assert!(s.input.is_empty());
        }
    }

    lazy_static! {
        static ref TEST_ACTION: Vec<(Action, Vec<u8>)> = vec![
            (
                Action::SetVar {
                    scope: Scope::Request,
                    name: "foo".into(),
                    value: "bar".into(),
                },
                {
                    let mut v = vec![SPOE_ACT_T_SET_VAR, 3, SPOE_SCOPE_REQ];
                    v.push(3);
                    v.extend_from_slice(b"foo");
                    v.push(SPOE_DATA_T_STR);
                    v.push(3);
                    v.extend_from_slice(b"bar");
                    v
                }
            ),
            (
                Action::UnsetVar {
                    scope: Scope::Response,
                    name: "foo".into(),
                },
                {
                    let mut v = vec![SPOE_ACT_T_UNSET_VAR, 2, SPOE_SCOPE_RES];
                    v.push(3);
                    v.extend_from_slice(b"foo");
                    v
                }
            ),
        ];
    }

    #[test]
    fn test_action() {
        for (a, b) in TEST_ACTION.iter() {
            assert_eq!(a.size(), b.len());

            let mut v = Vec::new();
            v.put_action(a.clone());
            assert_eq!(v.as_slice(), *b, "encode action: {:?}", a);

            let (r, s) = action().easy_parse(Stream::new(b.as_slice())).unwrap();
            assert_eq!(&r, a, "decode action: {:?}", b);
            assert!(s.input.is_empty());
        }
    }

    #[test]
    fn test_capabilities() {
        assert_eq!(
            capabilities().easy_parse(position::Stream::new("async,foobar,fragmentation")),
            Err(easy::Errors {
                position: position::SourcePosition { line: 1, column: 7 },
                errors: vec![easy::Error::Message("foobar".into())]
            })
        );
    }

    lazy_static! {
        static ref TEST_FRAME: Vec<(Frame, Vec<u8>)> = vec![
            (
                Frame::HaproxyHello(haproxy::Hello {
                    supported_versions: vec![Version::new(2, 0)],
                    max_frame_size: 1024,
                    capabilities: vec![Capability::Fragmentation, Capability::Async],
                    healthcheck: false,
                    engine_id: Some("foobar".into()),
                }),
                {
                    let mut v = vec![SPOE_FRM_T_HAPROXY_HELLO];
                    v.put_metadata(Metadata::default());
                    v.put_kv(SUPPORTED_VERSIONS_KEY, "2.0");
                    v.put_kv(MAX_FRAME_SIZE_KEY, 1024u32);
                    v.put_kv(CAPABILITIES_KEY, "fragmentation,async");
                    v.put_kv(ENGINE_ID_KEY, "foobar");
                    v
                }
            ),
            (
                Frame::AgentHello(agent::Hello {
                    version: Version::new(2, 0),
                    max_frame_size: 1024,
                    capabilities: vec![Capability::Fragmentation, Capability::Async],
                }),
                {
                    let mut v = vec![SPOE_FRM_T_AGENT_HELLO];
                    v.put_metadata(Metadata::default());
                    v.put_kv(VERSION_KEY, "2.0");
                    v.put_kv(MAX_FRAME_SIZE_KEY, 1024u32);
                    v.put_kv(CAPABILITIES_KEY, "fragmentation,async");
                    v
                }
            ),
            (
                Frame::HaproxyNotify(haproxy::Notify {
                    fragmented: false,
                    stream_id: 123,
                    frame_id: 456,
                    messages: vec![
                        Message {
                            name: "client".into(),
                            args: vec![
                                ("frontend".into(), "world".into()),
                                ("src".into(), Ipv4Addr::new(127, 0, 0, 1).into())
                            ]
                        },
                        Message {
                            name: "server".into(),
                            args: vec![
                                ("ip".into(), Ipv6Addr::LOCALHOST.into()),
                                ("port".into(), 80u32.into())
                            ],
                        }
                    ],
                }),
                {
                    let mut v = vec![SPOE_FRM_T_HAPROXY_NOTIFY];
                    v.put_metadata(Metadata {
                        flags: Flags::default(),
                        stream_id: 123,
                        frame_id: 456,
                    });

                    v.put_str("client");
                    v.put_u8(2);
                    v.put_kv("frontend", "world");
                    v.put_kv("src", Ipv4Addr::new(127, 0, 0, 1));

                    v.put_str("server");
                    v.put_u8(2);
                    v.put_kv("ip", Ipv6Addr::LOCALHOST);
                    v.put_kv("port", 80u32);

                    v
                }
            ),
            (
                Frame::AgentAck(agent::Ack {
                    fragmented: false,
                    stream_id: 123,
                    frame_id: 456,
                    actions: vec![
                        Action::SetVar {
                            scope: Scope::Request,
                            name: "foo".into(),
                            value: "bar".into(),
                        },
                        Action::UnsetVar {
                            scope: Scope::Response,
                            name: "foo".into(),
                        }
                    ]
                }),
                {
                    let mut v = vec![SPOE_FRM_T_AGENT_ACK];
                    v.put_metadata(Metadata {
                        flags: Flags::default(),
                        stream_id: 123,
                        frame_id: 456,
                    });

                    v.put_slice(&[SPOE_ACT_T_SET_VAR, 3, SPOE_SCOPE_REQ]);
                    v.put_kv("foo", "bar");

                    v.put_slice(&[SPOE_ACT_T_UNSET_VAR, 2, SPOE_SCOPE_RES]);
                    v.put_str("foo");

                    v
                }
            ),
            (
                Frame::HaproxyDisconnect(Disconnect {
                    status_code: Status::BadVersion as u32,
                    message: "bad version".into()
                }),
                {
                    let mut v = vec![SPOE_FRM_T_HAPROXY_DISCON];
                    v.put_metadata(Metadata::default());
                    v.put_kv(STATUS_CODE_KEY, Status::BadVersion as u32);
                    v.put_kv(MSG_KEY, "bad version");
                    v
                }
            ),
            (
                Frame::AgentDisconnect(Disconnect {
                    status_code: Status::BadFrameSize as u32,
                    message: "bad frame size".into()
                }),
                {
                    let mut v = vec![SPOE_FRM_T_AGENT_DISCON];
                    v.put_metadata(Metadata::default());
                    v.put_kv(STATUS_CODE_KEY, Status::BadFrameSize as u32);
                    v.put_kv(MSG_KEY, "bad frame size");
                    v
                }
            )
        ];
    }

    #[test]
    fn test_frame() {
        for (f, b) in TEST_FRAME.iter() {
            let mut v = Vec::new();
            v.put_frame(f.clone());
            assert_eq!(v.as_slice(), *b, "encode frame: {:?}", f);

            let (r, s) = frame().easy_parse(Stream::new(b.as_slice())).unwrap();
            assert_eq!(&r, f, "decode frame: {:?}", b);
            assert!(s.input.is_empty());

            assert_eq!(f.size(), b.len(), "frame: {:?}", r);
        }
    }
}
