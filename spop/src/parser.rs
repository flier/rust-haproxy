use std::convert::TryFrom;

use combine::{
    error::{ParseError, StreamError},
    parser::{
        byte::byte,
        choice::choice,
        range::{take, take_fn},
    },
    stream::{easy, position, Range, Stream, StreamErrorFor},
    EasyParser, Parser, RangeStreamOnce,
};

use crate::data::*;
use crate::varint::BufExt as _;

type PositionStream<'a> = position::Stream<&'a [u8], position::IndexPositioner>;

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
        byte(SPOE_DATA_T_UINT64)
            .with(varint())
            .map(|n| Data::Uint64(n)),
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
        byte(SPOE_DATA_T_STR)
            .with(varint())
            .then(|n| take(n as usize))
            .and_then(|b: Input::Range| {
                String::from_utf8(b.as_ref().to_vec())
                    .map(Data::String)
                    .map_err(StreamErrorFor::<Input>::other)
            }),
        byte(SPOE_DATA_T_BIN)
            .with(varint())
            .then(|n| take(n as usize))
            .map(|b: Input::Range| Data::Binary(b.as_ref().to_vec())),
    ))
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

    use super::{Data::*, *};

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
            v.put_data(&d);
            assert_eq!(v.as_slice(), *b, "encode data: {:?}", d);

            let (r, s) = Data::parse(b).unwrap();
            assert_eq!(r, d.clone(), "decode data: {:?}", b);
            assert!(s.input.is_empty());
        }
    }
}
