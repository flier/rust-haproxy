use std::collections::HashMap;
use std::fmt;
use std::iter::FromIterator;
use std::mem;
use std::net::{Ipv4Addr, Ipv6Addr};

use bytes::Bytes;
use derive_more::{Deref, From, Into, TryInto};

use crate::varint::{self, BufMutExt as _};

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
    Binary(Bytes),
}

impl From<&str> for Data {
    fn from(s: &str) -> Self {
        Data::String(s.to_string())
    }
}

impl<T: fmt::Display> From<Vec<T>> for Data {
    fn from(s: Vec<T>) -> Self {
        Data::String(
            s.into_iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    }
}

pub trait Value {
    fn size(&self) -> usize;
}

impl Value for bool {
    fn size(&self) -> usize {
        0
    }
}

impl Value for i32 {
    fn size(&self) -> usize {
        varint::size_of(*self as u64)
    }
}

impl Value for u32 {
    fn size(&self) -> usize {
        varint::size_of(*self as u64)
    }
}

impl Value for i64 {
    fn size(&self) -> usize {
        varint::size_of(*self as u64)
    }
}

impl Value for u64 {
    fn size(&self) -> usize {
        varint::size_of(*self)
    }
}

impl Value for Ipv4Addr {
    fn size(&self) -> usize {
        Data::IPV4_ADDR_LEN
    }
}

impl Value for Ipv6Addr {
    fn size(&self) -> usize {
        Data::IPV6_ADDR_LEN
    }
}

impl Value for String {
    fn size(&self) -> usize {
        varint::size_of(self.len() as u64) + self.len()
    }
}

impl Value for &str {
    fn size(&self) -> usize {
        varint::size_of(self.len() as u64) + self.len()
    }
}

impl Value for Bytes {
    fn size(&self) -> usize {
        varint::size_of(self.len() as u64) + self.len()
    }
}

impl<T: fmt::Display> Value for &[T] {
    fn size(&self) -> usize {
        let n = self.iter().map(|v| v.to_string().len() + 1).sum::<usize>() - 1;

        varint::size_of(n as u64) + n
    }
}

impl<T: Value> Value for (&'static str, T) {
    fn size(&self) -> usize {
        let (k, v) = self;
        varint::size_of(k.len() as u64) + k.len() + Data::TYPE_SIZE + v.size()
    }
}

impl Data {
    pub const IPV4_ADDR_LEN: usize = 4;
    pub const IPV6_ADDR_LEN: usize = 16;

    pub const TYPE_SIZE: usize = mem::size_of::<u8>();

    pub fn size(&self) -> usize {
        Self::TYPE_SIZE
            + match self {
                Data::Null => 0,
                Data::Boolean(b) => b.size(),
                Data::Int32(n) => n.size(),
                Data::Uint32(n) => n.size(),
                Data::Int64(n) => n.size(),
                Data::Uint64(n) => n.size(),
                Data::IPv4(a) => a.size(),
                Data::IPv6(a) => a.size(),
                Data::String(s) => s.size(),
                Data::Binary(v) => v.size(),
            }
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

    fn put_kv<V: Into<Data>>(&mut self, key: &'static str, value: V);
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

    fn put_kv<V: Into<Data>>(&mut self, key: &'static str, value: V) {
        self.put_str(key);
        self.put_data(value.into());
    }
}
