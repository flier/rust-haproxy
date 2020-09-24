use std::net::{Ipv4Addr, Ipv6Addr};

use derive_more::{From, TryInto};

use crate::varint::{self, BufMutExt as _};

pub const IPV4_ADDR_LEN: usize = 4;
pub const IPV6_ADDR_LEN: usize = 16;

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

/* Flags to set Boolean values */
pub const SPOE_DATA_FL_FALSE: u8 = 0x00;
pub const SPOE_DATA_FL_TRUE: u8 = 0x10;

/* All supported data types */
pub const SPOE_DATA_T_NULL: u8 = 0;
pub const SPOE_DATA_T_BOOL: u8 = 1;
pub const SPOE_DATA_T_INT32: u8 = 2;
pub const SPOE_DATA_T_UINT32: u8 = 3;
pub const SPOE_DATA_T_INT64: u8 = 4;
pub const SPOE_DATA_T_UINT64: u8 = 5;
pub const SPOE_DATA_T_IPV4: u8 = 6;
pub const SPOE_DATA_T_IPV6: u8 = 7;
pub const SPOE_DATA_T_STR: u8 = 8;
pub const SPOE_DATA_T_BIN: u8 = 9;

#[derive(Clone, Debug, PartialEq, From, TryInto)]
pub enum Data {
    Null,
    Boolean(bool),
    Int32(i32),
    Uint32(u32),
    Int64(i64),
    Uint64(u64),
    IPv4(Ipv4Addr),
    IPv6(Ipv6Addr),
    String(String),
    Binary(Vec<u8>),
}

pub fn size_of(data: &Data) -> usize {
    match data {
        Data::Null | Data::Boolean(_) => 1,
        Data::Int32(n) => 1 + varint::size_of(*n as u64),
        Data::Uint32(n) => 1 + varint::size_of(*n as u64),
        Data::Int64(n) => 1 + varint::size_of(*n as u64),
        Data::Uint64(n) => 1 + varint::size_of(*n as u64),
        Data::IPv4(_) => 1 + IPV4_ADDR_LEN,
        Data::IPv6(_) => 1 + IPV6_ADDR_LEN,
        Data::String(s) => 1 + varint::size_of(s.len() as u64) + s.len(),
        Data::Binary(v) => 1 + varint::size_of(v.len() as u64) + v.len(),
    }
}

pub trait BufMutExt {
    fn put_data(&mut self, data: &Data);
}

impl<T> BufMutExt for T
where
    T: bytes::BufMut,
{
    fn put_data(&mut self, data: &Data) {
        match data {
            Data::Null => self.put_u8(SPOE_DATA_T_NULL),
            Data::Boolean(b) => self.put_u8(
                SPOE_DATA_T_BOOL
                    | if *b {
                        SPOE_DATA_FL_TRUE
                    } else {
                        SPOE_DATA_FL_FALSE
                    },
            ),
            Data::Int32(n) => {
                self.put_u8(SPOE_DATA_T_INT32);
                self.put_varint(*n as u64);
            }
            Data::Uint32(n) => {
                self.put_u8(SPOE_DATA_T_UINT32);
                self.put_varint(*n as u64);
            }
            Data::Int64(n) => {
                self.put_u8(SPOE_DATA_T_INT64);
                self.put_varint(*n as u64);
            }
            Data::Uint64(n) => {
                self.put_u8(SPOE_DATA_T_UINT64);
                self.put_varint(*n);
            }
            Data::IPv4(addr) => {
                self.put_u8(SPOE_DATA_T_IPV4);
                self.put_slice(&addr.octets()[..]);
            }
            Data::IPv6(addr) => {
                self.put_u8(SPOE_DATA_T_IPV6);
                self.put_slice(&addr.octets()[..]);
            }
            Data::String(s) => {
                self.put_u8(SPOE_DATA_T_STR);
                self.put_varint(s.len() as u64);
                self.put_slice(s.as_bytes());
            }
            Data::Binary(v) => {
                self.put_u8(SPOE_DATA_T_BIN);
                self.put_varint(v.len() as u64);
                self.put_slice(&v);
            }
        }
    }
}
