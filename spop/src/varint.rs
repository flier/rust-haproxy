/*
Variable-length integer (varint) are encoded using Peers encoding:


       0  <= X < 240        : 1 byte  (7.875 bits)  [ XXXX XXXX ]
      240 <= X < 2288       : 2 bytes (11 bits)     [ 1111 XXXX ] [ 0XXX XXXX ]
     2288 <= X < 264432     : 3 bytes (18 bits)     [ 1111 XXXX ] [ 1XXX XXXX ]   [ 0XXX XXXX ]
   264432 <= X < 33818864   : 4 bytes (25 bits)     [ 1111 XXXX ] [ 1XXX XXXX ]*2 [ 0XXX XXXX ]
 33818864 <= X < 4328786160 : 5 bytes (32 bits)     [ 1111 XXXX ] [ 1XXX XXXX ]*3 [ 0XXX XXXX ]
 ...
*/

pub fn size_of(n: u64) -> usize {
    match n {
        _ if (0x0000000000000000..=0x00000000000000ef).contains(&n) => 1,
        _ if (0x00000000000000f0..=0x00000000000008ef).contains(&n) => 2,
        _ if (0x00000000000008f0..=0x00000000000408ef).contains(&n) => 3,
        _ if (0x00000000000408f0..=0x00000000020408ef).contains(&n) => 4,
        _ if (0x00000000020408f0..=0x00000001020408ef).contains(&n) => 5,
        _ if (0x00000001020408f0..=0x00000081020408ef).contains(&n) => 6,
        _ if (0x00000081020408f0..=0x00004081020408ef).contains(&n) => 7,
        _ if (0x00004081020408f0..=0x00204081020408ef).contains(&n) => 8,
        _ if (0x00204081020408f0..=0x10204081020408ef).contains(&n) => 9,
        _ => 10,
    }
}

pub trait BufExt {
    fn get_varint(&mut self) -> u64;
}

impl<T> BufExt for T
where
    T: bytes::Buf,
{
    fn get_varint(&mut self) -> u64 {
        let mut b = self.get_u8();
        if b < 0xF0 {
            b as u64
        } else {
            let mut n = b as u64;
            let mut r = 4;
            while self.has_remaining() {
                b = self.get_u8();
                n += (b as u64) << r;
                r += 7;

                if b < 0x80 {
                    break;
                }
            }
            n
        }
    }
}

pub trait BufMutExt {
    fn put_varint(&mut self, n: u64);
}

impl<T> BufMutExt for T
where
    T: bytes::BufMut,
{
    fn put_varint(&mut self, mut n: u64) {
        assert!(self.remaining_mut() >= size_of(n));

        if n < 0xF0 {
            self.put_u8(n as u8);
        } else {
            self.put_u8((n as u8) | 0xF0);
            n = (n - 0xF0) >> 4;
            while n >= 0x80 {
                self.put_u8((n as u8) | 0x80);
                n = (n - 0x80) >> 7;
            }
            self.put_u8(n as u8);
        }
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
            v.put_varint(n);
            assert_eq!(v.as_slice(), b);

            assert_eq!(b.get_varint(), n);
        }
    }
}
