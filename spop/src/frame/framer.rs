use bytes::{BufMut, BytesMut};
use futures::pin_mut;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{
    error::{Error::*, Result},
    frame::{BufExt, BufMutExt, Frame},
};

#[derive(Clone, Debug)]
pub struct Framer {
    max_frame_size: u32,
}

impl Framer {
    pub fn new(max_frame_size: u32) -> Framer {
        Framer { max_frame_size }
    }

    pub async fn read_frame<R>(&self, r: R) -> Result<Frame>
    where
        R: AsyncRead + Sized,
    {
        pin_mut!(r);

        let len = r.read_u32().await.map_err(|_| Io)?;
        if len <= self.max_frame_size {
            let mut buf = {
                let mut buf = BytesMut::with_capacity(self.max_frame_size as usize);
                buf.resize(len as usize, 0);
                r.read_exact(&mut buf).await.map_err(|_| Io)?;
                buf.freeze()
            };

            buf.get_frame().map_err(|_| Invalid)
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
            buf.put_frame(frame);
            buf.freeze()
        };

        pin_mut!(w);

        w.write_all(&buf).await.map_err(|_| Io)?;

        Ok(buf.len())
    }
}
