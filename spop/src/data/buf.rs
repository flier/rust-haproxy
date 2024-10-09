use std::convert::TryFrom;
use std::iter;
use std::net::{Ipv4Addr, Ipv6Addr};

use bytes::{Buf, BufMut, Bytes};
use num_enum::TryFromPrimitive;

use crate::data::{varint, Flags, KeyValue, Type, Typed};

/// Read data types from a buffer.
pub trait BufExt {
    /// Get a typed value.
    fn typed(&mut self) -> Option<Typed>;

    /// Get a varint value.
    fn varint(&mut self) -> Option<u64>;

    /// Get a string.
    fn string(&mut self) -> Option<String>;

    /// Get key-value list.
    fn kv_list(&mut self) -> impl Iterator<Item = (String, Typed)>;
}

impl<T> BufExt for T
where
    T: Buf,
{
    fn typed(&mut self) -> Option<Typed> {
        typed_data(self)
    }

    fn varint(&mut self) -> Option<u64> {
        varint::get(self)
    }

    fn string(&mut self) -> Option<String> {
        let sz = self.varint()?;
        let b = get_bytes(self, sz as usize)?;
        String::from_utf8(b.to_vec()).ok()
    }

    fn kv_list(&mut self) -> impl Iterator<Item = (String, Typed)> {
        iter::from_fn(move || {
            if self.has_remaining() {
                let name = self.string()?;
                let value = self.typed()?;

                Some((name, value))
            } else {
                None
            }
        })
    }
}

fn get_bytes<T: Buf>(mut buf: T, n: usize) -> Option<Bytes> {
    (buf.remaining() >= n).then(|| buf.copy_to_bytes(n))
}

fn typed_data<B: Buf>(mut buf: B) -> Option<Typed> {
    let (ty, flags) = typed_data_type(&mut buf)?;

    match ty {
        Type::Null => Some(Typed::Null),
        Type::Boolean => Some(Typed::Boolean(flags.contains(Flags::TRUE))),
        Type::Int32 => buf.varint().map(|n| n as i32).map(Typed::Int32),
        Type::Uint32 => buf.varint().map(|n| n as u32).map(Typed::Uint32),
        Type::Int64 => buf.varint().map(|n| n as i64).map(Typed::Int64),
        Type::Uint64 => buf.varint().map(Typed::Uint64),
        Type::Ipv4 => get_bytes(buf, Typed::IPV4_ADDR_LEN)
            .map(|b| <[u8; Typed::IPV4_ADDR_LEN]>::try_from(&b[..]).unwrap())
            .map(Ipv4Addr::from)
            .map(Typed::Ipv4),
        Type::Ipv6 => get_bytes(buf, Typed::IPV6_ADDR_LEN)
            .map(|b| <[u8; Typed::IPV6_ADDR_LEN]>::try_from(&b[..]).unwrap())
            .map(Ipv6Addr::from)
            .map(Typed::Ipv6),
        Type::String => buf.string().map(Typed::String),
        Type::Binary => buf
            .varint()
            .and_then(|n| get_bytes(buf, n as usize))
            .map(Typed::Binary),
    }
}

fn typed_data_type<B: Buf>(mut buf: B) -> Option<(Type, Flags)> {
    let b = buf.has_remaining().then(|| buf.get_u8())?;
    let ty = Type::try_from_primitive(b & Type::MASK).ok()?;
    let flags = Flags::from_bits_truncate(b & Flags::MASK);

    Some((ty, flags))
}

/// Writes data types to a buffer.
pub trait BufMutExt {
    /// Writes value in typed data format.
    fn put_typed<D: Into<Typed>>(&mut self, data: D) -> usize;

    /// Writes value in varint format.
    fn put_varint(&mut self, n: u64) -> usize;

    /// Writes a string with length.
    fn put_string<S: AsRef<str>>(&mut self, s: S) -> usize;

    /// Writes a key-value pair.
    fn put_kv<'a, KV: Into<KeyValue<'a, V>>, V: Into<Typed>>(&mut self, kv: KV) -> usize {
        let KeyValue(key, value) = kv.into();

        self.put_string(key) + self.put_typed(value.into())
    }

    /// Writes a key-value list.
    fn put_kvlist<'a, I, KV, V>(&mut self, i: I) -> usize
    where
        I: IntoIterator<Item = KV>,
        KV: Into<KeyValue<'a, V>>,
        V: Into<Typed>,
    {
        let mut sz = 0;

        for kv in i {
            sz += self.put_kv(kv.into());
        }

        sz
    }
}

impl<T> BufMutExt for T
where
    T: BufMut,
{
    fn put_typed<D: Into<Typed>>(&mut self, data: D) -> usize {
        match data.into() {
            Typed::Null => {
                self.put_u8(Type::Null as u8);

                Typed::TYPE_SIZE
            }
            Typed::Boolean(b) => {
                self.put_u8(
                    Type::Boolean as u8
                        | if b {
                            Flags::TRUE.bits()
                        } else {
                            Flags::FALSE.bits()
                        },
                );

                Typed::TYPE_SIZE
            }
            Typed::Int32(n) => {
                self.put_u8(Type::Int32 as u8);
                self.put_varint(n as u64) + Typed::TYPE_SIZE
            }
            Typed::Uint32(n) => {
                self.put_u8(Type::Uint32 as u8);
                self.put_varint(n as u64) + Typed::TYPE_SIZE
            }
            Typed::Int64(n) => {
                self.put_u8(Type::Int64 as u8);
                self.put_varint(n as u64) + Typed::TYPE_SIZE
            }
            Typed::Uint64(n) => {
                self.put_u8(Type::Uint64 as u8);
                self.put_varint(n) + Typed::TYPE_SIZE
            }
            Typed::Ipv4(addr) => {
                self.put_u8(Type::Ipv4 as u8);
                self.put_slice(&addr.octets()[..]);
                Typed::TYPE_SIZE + Typed::IPV4_ADDR_LEN
            }
            Typed::Ipv6(addr) => {
                self.put_u8(Type::Ipv6 as u8);
                self.put_slice(&addr.octets()[..]);
                Typed::TYPE_SIZE + Typed::IPV6_ADDR_LEN
            }
            Typed::String(s) => {
                self.put_u8(Type::String as u8);
                self.put_string(&s) + Typed::TYPE_SIZE
            }
            Typed::Binary(b) => {
                self.put_u8(Type::Binary as u8);
                let sz = self.put_varint(b.len() as u64);
                self.put_slice(&b);
                sz + b.len() + Typed::TYPE_SIZE
            }
        }
    }

    fn put_varint(&mut self, n: u64) -> usize {
        varint::put(self, n)
    }

    fn put_string<S: AsRef<str>>(&mut self, s: S) -> usize {
        put_bytes(self, s.as_ref().as_bytes())
    }
}

fn put_bytes<T: BufMut, B: AsRef<[u8]>>(mut buf: T, b: B) -> usize {
    let b = b.as_ref();
    let sz = buf.put_varint(b.len() as u64);
    buf.put_slice(b);
    sz + b.len()
}

#[cfg(test)]
mod tests {
    use crate::data::{Type, Typed::*};

    use super::*;

    pub const TRUE: u8 = Type::Boolean as u8 | Flags::TRUE.bits();
    pub const FALSE: u8 = Type::Boolean as u8 | Flags::FALSE.bits();

    #[test]
    fn test_typed_data() {
        let values = [
            (Null, &[Type::Null as u8][..]),
            (Boolean(true), &[TRUE][..]),
            (Boolean(false), &[FALSE][..]),
            (Int32(123), &[Type::Int32 as u8, 123][..]),
            (Uint32(456), &[Type::Uint32 as u8, 0xf8, 0x0d][..]),
            (Int64(789), &[Type::Int64 as u8, 0xf5, 0x22][..]),
            (Uint64(999), &[Type::Uint64 as u8, 0xf7, 0x2f][..]),
            (
                Ipv4(Ipv4Addr::new(127, 0, 0, 1)),
                &[Type::Ipv4 as u8, 127, 0, 0, 1],
            ),
            (
                Ipv6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc00a, 0x2ff)),
                &[
                    Type::Ipv6 as u8,
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
                b"\x09\x0bhello world",
            ),
        ];

        for (v, b) in values {
            let mut buf = Vec::new();
            assert_eq!(
                buf.put_typed(v.clone()),
                b.len(),
                "put_typed({v:?}) -> {b:?}"
            );
            assert_eq!(buf.as_slice(), b, "put_typed({v:?}) -> {b:?}");

            assert_eq!(
                buf.as_slice().typed(),
                Some(v.clone()),
                "get_typed({b:?}) -> {v:?}"
            );
        }
    }
}
