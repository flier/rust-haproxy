use std::iter;

use bytes::Buf;
use http::{HeaderMap, HeaderName, HeaderValue};

use crate::error::Result;

pub fn hdrs_bin<T: Buf>(mut b: T) -> Result<HeaderMap> {
    let mut hdrs = HeaderMap::new();

    let mut strs = iter::from_fn(|| {
        if b.has_remaining() {
            let len = b.get_u8() as usize;
            if b.remaining() >= len {
                return Some(b.copy_to_bytes(len));
            }
        }

        None
    });

    while let Some((k, v)) = strs.next().zip(strs.next()) {
        if k.is_empty() && v.is_empty() {
            break;
        }

        hdrs.insert(HeaderName::from_bytes(&k)?, HeaderValue::from_bytes(&v)?);
    }

    Ok(hdrs)
}
