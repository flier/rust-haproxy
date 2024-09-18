//! Variable-length integer
//!
//! Variable-length integer (varint) are encoded using Peers encoding:
//!
//! | Range                      | Size                 | Encoding
//! |----------------------------|----------------------|--------------------------------------------
//! |       0  <= X < 240        | 1 byte  (7.875 bits) | [ XXXX XXXX ]
//! |      240 <= X < 2288       | 2 bytes (11 bits)    | [ 1111 XXXX ] [ 0XXX XXXX ]
//! |     2288 <= X < 264432     | 3 bytes (18 bits)    | [ 1111 XXXX ] [ 1XXX XXXX ]   [ 0XXX XXXX ]
//! |   264432 <= X < 33818864   | 4 bytes (25 bits)    | [ 1111 XXXX ] [ 1XXX XXXX ]*2 [ 0XXX XXXX ]
//! | 33818864 <= X < 4328786160 | 5 bytes (32 bits)    | [ 1111 XXXX ] [ 1XXX XXXX ]*3 [ 0XXX XXXX ]

use bytes::{Buf, BufMut};

/// Get a varint from the buffer.
pub fn get<T: Buf>(mut buf: T) -> Option<u64> {
    if !buf.has_remaining() {
        None
    } else {
        let b = buf.get_u8();

        if b < 0xF0 {
            Some(b as u64)
        } else {
            let mut n = b as u64;
            let mut r = 4;

            while buf.has_remaining() {
                let b = buf.get_u8();
                n += (b as u64) << r;
                r += 7;

                if b < 0x80 {
                    break;
                }
            }

            Some(n)
        }
    }
}

/// Writes a varint to the buffer.
pub fn put<T: BufMut>(mut buf: T, mut n: u64) -> usize {
    let mut sz = 1;

    if n < 0xF0 {
        buf.put_u8(n as u8);
    } else {
        buf.put_u8((n as u8) | 0xF0);
        n = (n - 0xF0) >> 4;
        while n >= 0x80 {
            buf.put_u8((n as u8) | 0x80);
            n = (n - 0x80) >> 7;
            sz += 1;
        }
        buf.put_u8(n as u8);
        sz += 1;
    }

    sz
}

/// Returns the size of varint.
pub const fn size_of(n: u64) -> usize {
    match n {
        ..0x00000000000000f0 => 1,
        0x00000000000000f0..0x00000000000008f0 => 2,
        0x00000000000008f0..0x00000000000408f0 => 3,
        0x00000000000408f0..0x00000000020408f0 => 4,
        0x00000000020408f0..0x00000001020408f0 => 5,
        0x00000001020408f0..0x00000081020408f0 => 6,
        0x00000081020408f0..0x00004081020408f0 => 7,
        0x00004081020408f0..0x00204081020408f0 => 8,
        0x00204081020408f0..0x10204081020408f0 => 9,
        _ => 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DATA: &[(u64, &[u8])] = &[
        (0, &[0]),
        (1, &[1]),
        (240, &[0xF0, 0x00]),
        (0x0000_08ef, &[0xff, 0x7f]),
        (0x0004_08ef, &[0xff, 0xff, 0x7f]),
        (0x0204_08ef, &[0xff, 0xff, 0xff, 0x7f]),
        (0x0001_0204_08ef, &[0xff, 0xff, 0xff, 0xff, 0x7f]),
        (0x0081_0204_08ef, &[0xff, 0xff, 0xff, 0xff, 0xff, 0x7f]),
        (
            0x4081_0204_08ef,
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f],
        ),
        (
            0x0020_4081_0204_08ef,
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f],
        ),
        (
            0x01020_4081_0204_08ef,
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f],
        ),
        (
            0xffff_ffff_ffff_ffff,
            &[0xff, 0xf0, 0xfe, 0xfe, 0xfe, 0xfe, 0xfe, 0xfe, 0xfe, 0x0e],
        ),
    ];

    #[test]
    fn test_varint() {
        for &(n, mut b) in TEST_DATA {
            assert_eq!(size_of(n), b.len());

            let mut v = Vec::new();

            assert_eq!(put(&mut v, n), b.len(), "encode {n} to: {b:?}");
            assert_eq!(v.as_slice(), b);

            assert_eq!(get(&mut b).unwrap(), n);
        }
    }
}
