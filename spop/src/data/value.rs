use std::borrow::Cow;
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

use bytes::Bytes;
use derive_more::Into;

use crate::data::{typed, varint, Typed};

/// The Value can be used in a KV-list.
pub trait Value: Sized {
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
        Typed::IPV4_ADDR_LEN
    }
}

impl Value for Ipv6Addr {
    fn size(&self) -> usize {
        Typed::IPV6_ADDR_LEN
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

impl<'a> Value for Cow<'a, str> {
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

        k.size() + typed::size_of_val(v)
    }
}

/// The Key-Value pair can be used in a KV-list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyValue<'a, T>(pub(crate) Cow<'a, str>, pub(crate) T);

impl<T> From<(&'static str, T)> for KeyValue<'static, T> {
    fn from((key, value): (&'static str, T)) -> Self {
        KeyValue(key.into(), value)
    }
}

impl<T> From<(String, T)> for KeyValue<'_, T> {
    fn from((key, value): (String, T)) -> Self {
        KeyValue(key.into(), value)
    }
}

impl<'a, T> Value for KeyValue<'a, T>
where
    T: Value,
{
    fn size(&self) -> usize {
        self.0.size() + typed::size_of_val(&self.1)
    }
}
