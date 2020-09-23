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
            loop {
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
