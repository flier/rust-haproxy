use bytes::{BufMut, BytesMut};
use futures::pin_mut;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{
    error::{Error::*, Result},
    frame::{parse_frame, put_frame, Frame},
};

#[derive(Clone, Debug)]
pub struct Framer {
    max_frame_size: usize,
}

impl Framer {
    pub fn new(max_frame_size: usize) -> Framer {
        Framer { max_frame_size }
    }

    pub async fn read_frame<R>(&self, r: R) -> Result<Frame>
    where
        R: AsyncRead + Sized,
    {
        pin_mut!(r);

        let len = r.read_u32().await.map_err(|_| Io)? as usize;
        if len <= self.max_frame_size {
            let buf = {
                let mut buf = BytesMut::with_capacity(self.max_frame_size);
                buf.resize(len, 0);
                r.read_exact(&mut buf).await.map_err(|_| Io)?;
                buf.freeze()
            };

            parse_frame(buf).map_err(|_| Invalid)
        } else {
            Err(BadFrameSize)
        }
    }

    pub async fn write_frame<W>(&self, w: W, frame: Frame) -> Result<usize>
    where
        W: AsyncWrite + Sized,
    {
        let buf = {
            let len = frame.size();
            let mut buf = BytesMut::with_capacity(Frame::LENGTH_SIZE + len);
            buf.put_u32(len as u32);
            put_frame(&mut buf, frame);
            buf.freeze()
        };

        pin_mut!(w);

        w.write_all(&buf).await.map_err(|_| Io)?;

        Ok(buf.len())
    }
}
