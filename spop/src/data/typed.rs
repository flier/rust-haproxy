use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use bytes::{Bytes, BytesMut};
use derive_more::{From, TryInto};

/// Typed data
///
/// Here is the bytewise representation of typed data:
///
/// > TYPED-DATA    : <TYPE:4 bits><FLAGS:4 bits><DATA>
///
/// Supported types and their representation are:
///
/// |     TYPE                      |  ID | DESCRIPTION
/// |-------------------------------|-----|----------------------------------
/// |     NULL                      |  0  |  NULL   : < 0 >
/// |     Boolean                   |  1  |  BOOL   : < 1+FLAG >
/// |     32bits signed integer     |  2  |  INT32  : < 2 > < VALUE:varint >
/// |     32bits unsigned integer   |  3  |  UINT32 : < 3 > < VALUE:varint >
/// |     64bits signed integer     |  4  |  INT64  : < 4 > < VALUE:varint >
/// |     32bits unsigned integer   |  5  |  UNIT64 : < 5 > < VALUE:varint >
/// |     IPV4                      |  6  |  IPV4   : < 6 > < STRUCT IN_ADDR:4 bytes >
/// |     IPV6                      |  7  |  IPV6   : < 7 > < STRUCT IN_ADDR6:16 bytes >
/// |     String                    |  8  |  STRING : < 8 > < LENGTH:varint > < BYTES >
/// |     Binary                    |  9  |  BINARY : < 9 > < LENGTH:varint > < BYTES >
/// |    10 -> 15  unused/reserved  |  -  |  -
#[derive(Clone, Debug, PartialEq, Eq, From, TryInto)]
pub enum Typed {
    /// Null value
    Null,
    /// Boolean type
    Boolean(bool),
    /// 32bits signed integer   
    Int32(i32),
    /// 32bits unsigned integer   
    Uint32(u32),
    /// 64bits signed integer
    Int64(i64),
    /// 64bits unsigned integer
    Uint64(u64),
    /// IPv4 address
    Ipv4(Ipv4Addr),
    /// IPv6 address
    Ipv6(Ipv6Addr),
    /// String type
    String(String),
    /// Binary type
    Binary(Bytes),
}

impl From<()> for Typed {
    fn from(_: ()) -> Self {
        Typed::Null
    }
}

impl From<&str> for Typed {
    fn from(s: &str) -> Self {
        Typed::String(s.to_string())
    }
}

impl<'a> From<&'a [u8]> for Typed {
    fn from(b: &'a [u8]) -> Self {
        Typed::Binary(Bytes::copy_from_slice(b))
    }
}

impl From<Box<[u8]>> for Typed {
    fn from(buf: Box<[u8]>) -> Self {
        Typed::Binary(buf.into())
    }
}

impl From<Vec<u8>> for Typed {
    fn from(buf: Vec<u8>) -> Self {
        Typed::Binary(buf.into())
    }
}

impl From<BytesMut> for Typed {
    fn from(buf: BytesMut) -> Self {
        Typed::Binary(buf.freeze())
    }
}

impl From<IpAddr> for Typed {
    fn from(addr: IpAddr) -> Self {
        match addr {
            IpAddr::V4(v4) => Typed::Ipv4(v4),
            IpAddr::V6(v6) => Typed::Ipv6(v6),
        }
    }
}

impl Typed {
    pub(crate) const IPV4_ADDR_LEN: usize = 4;
    pub(crate) const IPV6_ADDR_LEN: usize = 16;

    pub const TYPE_SIZE: usize = 1;
}
