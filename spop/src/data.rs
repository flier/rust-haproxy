use std::collections::HashMap;
use std::iter::FromIterator;
use std::net::{Ipv4Addr, Ipv6Addr};

use derive_more::{Deref, From, Into, TryInto};

use crate::varint::{self, BufMutExt as _};

pub const IPV4_ADDR_LEN: usize = 4;
pub const IPV6_ADDR_LEN: usize = 16;

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

impl From<&str> for Data {
    fn from(s: &str) -> Self {
        Data::String(s.to_string())
    }
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

#[derive(Clone, Debug, PartialEq, Deref, From, Into)]
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

pub trait BufMutExt {
    fn put_data(&mut self, data: Data);

    fn put_str<S: AsRef<str>>(&mut self, s: S);

    fn put_kvlist<K: AsRef<str>>(&mut self, kvlist: impl IntoIterator<Item = (K, Data)>);

    fn put_kv<K: AsRef<str>, V: Into<Data>>(&mut self, key: K, value: V);
}

impl<T> BufMutExt for T
where
    T: bytes::BufMut,
{
    fn put_data(&mut self, data: Data) {
        match data {
            Data::Null => self.put_u8(SPOE_DATA_T_NULL),
            Data::Boolean(b) => self.put_u8(
                SPOE_DATA_T_BOOL
                    | if b {
                        SPOE_DATA_FL_TRUE
                    } else {
                        SPOE_DATA_FL_FALSE
                    },
            ),
            Data::Int32(n) => {
                self.put_u8(SPOE_DATA_T_INT32);
                self.put_varint(n as u64);
            }
            Data::Uint32(n) => {
                self.put_u8(SPOE_DATA_T_UINT32);
                self.put_varint(n as u64);
            }
            Data::Int64(n) => {
                self.put_u8(SPOE_DATA_T_INT64);
                self.put_varint(n as u64);
            }
            Data::Uint64(n) => {
                self.put_u8(SPOE_DATA_T_UINT64);
                self.put_varint(n);
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
                self.put_str(&s);
            }
            Data::Binary(v) => {
                self.put_u8(SPOE_DATA_T_BIN);
                self.put_varint(v.len() as u64);
                self.put_slice(&v);
            }
        }
    }

    fn put_str<S: AsRef<str>>(&mut self, s: S) {
        let s = s.as_ref();
        self.put_varint(s.len() as u64);
        self.put_slice(s.as_bytes());
    }

    fn put_kvlist<K: AsRef<str>>(&mut self, kvlist: impl IntoIterator<Item = (K, Data)>) {
        for (key, value) in kvlist {
            self.put_str(key);
            self.put_data(value);
        }
    }

    fn put_kv<K: AsRef<str>, V: Into<Data>>(&mut self, key: K, value: V) {
        self.put_str(key);
        self.put_data(value.into());
    }
}
