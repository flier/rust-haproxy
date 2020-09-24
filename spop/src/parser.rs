use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use combine::{
    any, count_min_max,
    error::{ParseError, StreamError},
    from_str, many1,
    parser::{
        byte::byte,
        char::{char, digit, spaces},
        choice::choice,
        range::{take, take_fn},
    },
    sep_by,
    stream::{easy, position, Range, Stream, StreamErrorFor},
    EasyParser, Parser, RangeStreamOnce,
};

use crate::{action::*, data::*, frame::*, varint::BufExt};

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

pub fn frame<Input>() -> impl Parser<Input, Output = Frame>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    metadata().then(|metadata| {
        choice((
            byte(SPOE_FRM_T_HAPROXY_HELLO)
                .with(haproxy_hello())
                .map(Frame::HaproxyHello),
            byte(SPOE_FRM_T_HAPROXY_DISCON)
                .with(disconnect())
                .map(Frame::HaproxyDisconnect),
            byte(SPOE_FRM_T_HAPROXY_NOTIFY)
                .with(haproxy_notify(&metadata))
                .map(Frame::HaproxyNotify),
            byte(SPOE_FRM_T_AGENT_HELLO)
                .with(agent_hello())
                .map(Frame::AgentHello),
            byte(SPOE_FRM_T_AGENT_DISCON)
                .with(disconnect())
                .map(Frame::AgentDisconnect),
            byte(SPOE_FRM_T_AGENT_ACK)
                .with(agent_ack(&metadata))
                .map(Frame::AgentAck),
        ))
    })
}

pub fn metadata<Input>() -> impl Parser<Input, Output = Metadata>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    let flags = take(4).and_then(|b: Input::Range| {
        <[u8; 4]>::try_from(b.as_ref())
            .map(u32::from_be_bytes)
            .map(Flags::from_bits_truncate)
            .map_err(StreamErrorFor::<Input>::other)
    });

    (flags, varint(), varint()).map(|(flags, stream_id, frame_id)| Metadata {
        flags,
        stream_id,
        frame_id,
    })
}

pub fn kvlist<Input>() -> impl Parser<Input, Output = KVList>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    many1((string(), data())).map(KVList)
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

pub fn haproxy_hello<Input>() -> impl Parser<Input, Output = haproxy::Hello>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    kvlist().and_then::<_, _, StreamErrorFor<Input>>(|mut kvs| {
        Ok(haproxy::Hello {
            supported_versions: kvs
                .extract_into::<String, _, _, _>(SUPPORTED_VERSIONS_KEY)?
                .parse::<Versions>()
                .map(|versions| versions.0)
                .map_err(StreamErrorFor::<Input>::unexpected_format)?,
            max_frame_size: kvs.extract_into(MAX_FRAME_SIZE_KEY)?,
            capabilities: kvs
                .extract_into::<String, _, _, _>(CAPABILITIES_KEY)?
                .split(", \t")
                .flat_map(|s| s.parse().ok())
                .collect::<Vec<_>>(),
            healthcheck: kvs.extract(HEALTHCHECK_KEY).unwrap_or_default(),
            engine_id: kvs.extract(ENGINE_ID_KEY),
        })
    })
}

pub fn agent_hello<Input>() -> impl Parser<Input, Output = agent::Hello>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    kvlist().and_then::<_, _, StreamErrorFor<Input>>(|mut kvs| {
        Ok(agent::Hello {
            version: kvs
                .extract_into::<String, _, _, _>(VERSION_KEY)?
                .parse::<Version>()
                .map_err(StreamErrorFor::<Input>::unexpected_format)?,
            max_frame_size: kvs.extract_into(MAX_FRAME_SIZE_KEY)?,
            capabilities: kvs
                .extract_into::<String, _, _, _>(CAPABILITIES_KEY)?
                .split(", \t")
                .flat_map(|s| s.parse().ok())
                .collect::<Vec<_>>(),
        })
    })
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

struct Versions(Vec<Version>);

impl FromStr for Versions {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        versions()
            .easy_parse(position::Stream::new(s))
            .map(|r| Versions(r.0))
            .map_err(|err| err.to_string())
    }
}

impl FromStr for Version {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        version()
            .easy_parse(position::Stream::new(s))
            .map(|r| r.0)
            .map_err(|err| err.to_string())
    }
}

fn versions<Input>() -> impl Parser<Input, Output = Vec<Version>>
where
    Input: Stream<Token = char>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    sep_by(version(), spaces().skip(char(',')))
}

fn version<Input>() -> impl Parser<Input, Output = Version>
where
    Input: Stream<Token = char>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    (
        from_str(many1::<String, _, _>(digit())),
        char('.'),
        from_str(many1::<String, _, _>(digit())),
    )
        .map(|(major, _, minor)| Version { major, minor })
}

pub fn haproxy_notify<Input>(metadata: &Metadata) -> impl Parser<Input, Output = haproxy::Notify>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    let stream_id = metadata.stream_id;
    let frame_id = metadata.frame_id;

    many1::<Vec<_>, _, _>(message()).map(move |messages| haproxy::Notify {
        fragmented: false,
        stream_id,
        frame_id,
        messages,
    })
}

pub fn message<Input>() -> impl Parser<Input, Output = Message>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    (
        string(),
        any().then(|nb| {
            count_min_max::<HashMap<_, _>, _, _>(nb as usize, nb as usize, (string(), data()))
        }),
    )
        .map(|(name, args)| Message { name, args })
}

pub fn agent_ack<Input>(metadata: &Metadata) -> impl Parser<Input, Output = agent::Ack>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    let stream_id = metadata.stream_id;
    let frame_id = metadata.frame_id;

    many1::<Vec<_>, _, _>(action()).map(move |actions| agent::Ack {
        fragmented: false,
        stream_id,
        frame_id,
        actions,
    })
}

pub fn action<Input>() -> impl Parser<Input, Output = Action>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    choice((
        // SET-VAR requires 3 arguments
        (byte(SPOE_ACT_T_SET_VAR), byte(3), scope(), string(), data())
            .map(|(_, _, scope, name, value)| Action::SetVar { scope, name, value }),
        // UNSET-VAR requires 2 arguments
        (byte(SPOE_ACT_T_UNSET_VAR), byte(2), scope(), string())
            .map(|(_, _, scope, name)| Action::UnsetVar { scope, name }),
    ))
}

pub fn scope<Input>() -> impl Parser<Input, Output = Scope>
where
    Input: Stream<Token = u8>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    choice((
        byte(SPOE_SCOPE_PROC).map(|_| Scope::Process),
        byte(SPOE_SCOPE_SESS).map(|_| Scope::Session),
        byte(SPOE_SCOPE_TXN).map(|_| Scope::Transaction),
        byte(SPOE_SCOPE_REQ).map(|_| Scope::Request),
        byte(SPOE_SCOPE_RES).map(|_| Scope::Response),
    ))
}

pub fn disconnect<Input>() -> impl Parser<Input, Output = Disconnect>
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

impl Data {
    pub fn parse(b: &[u8]) -> Result<(Data, PositionStream), easy::ParseError<PositionStream>> {
        data().easy_parse(position::Stream::new(b))
    }
}

pub fn data<Input>() -> impl Parser<Input, Output = Data>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    choice((
        byte(SPOE_DATA_T_NULL).map(|_| Data::Null),
        byte(SPOE_DATA_T_BOOL | SPOE_DATA_FL_FALSE).map(|_| Data::Boolean(false)),
        byte(SPOE_DATA_T_BOOL | SPOE_DATA_FL_TRUE).map(|_| Data::Boolean(true)),
        byte(SPOE_DATA_T_INT32)
            .with(varint())
            .map(|n| Data::Int32(n as i32)),
        byte(SPOE_DATA_T_UINT32)
            .with(varint())
            .map(|n| Data::Uint32(n as u32)),
        byte(SPOE_DATA_T_INT64)
            .with(varint())
            .map(|n| Data::Int64(n as i64)),
        byte(SPOE_DATA_T_UINT64).with(varint()).map(Data::Uint64),
        byte(SPOE_DATA_T_IPV4)
            .with(take(IPV4_ADDR_LEN))
            .and_then(|b: Input::Range| {
                <[u8; IPV4_ADDR_LEN]>::try_from(b.as_ref())
                    .map(Ipv4Addr::from)
                    .map(Data::IPv4)
                    .map_err(StreamErrorFor::<Input>::other)
            }),
        byte(SPOE_DATA_T_IPV6)
            .with(take(IPV6_ADDR_LEN))
            .and_then(|b: Input::Range| {
                <[u8; IPV6_ADDR_LEN]>::try_from(b.as_ref())
                    .map(Ipv6Addr::from)
                    .map(Data::IPv6)
                    .map_err(StreamErrorFor::<Input>::other)
            }),
        byte(SPOE_DATA_T_STR).with(string()).map(Data::String),
        byte(SPOE_DATA_T_BIN).with(binary()).map(Data::Binary),
    ))
}

pub fn string<Input>() -> impl Parser<Input, Output = String>
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
}

pub fn binary<Input>() -> impl Parser<Input, Output = Vec<u8>>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    varint()
        .then(|n| take(n as usize))
        .map(|b: Input::Range| b.as_ref().to_vec())
}

fn varint<Input>() -> impl Parser<Input, Output = u64>
where
    Input: Stream<Token = u8> + RangeStreamOnce,
    Input::Range: Range + AsRef<[u8]>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    take_fn(|b: Input::Range| b.as_ref().iter().position(|&b| b < 0x80).map(|n| n + 1))
        .map(|b: Input::Range| b.as_ref().get_varint())
}

#[cfg(test)]
mod tests {
    use lazy_static::lazy_static;

    use super::*;
    use crate::{data::BufMutExt, Data::*};

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
            (Binary(b"hello world".to_vec()), b"\x09\x0bhello world"),
        ]
        .to_vec();
    }

    #[test]
    fn test_data() {
        for (d, b) in TEST_DATA.iter() {
            assert_eq!(size_of(d), b.len());

            let mut v = Vec::new();
            v.put_data(d.clone());
            assert_eq!(v.as_slice(), *b, "encode data: {:?}", d);

            let (r, s) = Data::parse(b).unwrap();
            assert_eq!(r, d.clone(), "decode data: {:?}", b);
            assert!(s.input.is_empty());
        }
    }
}
