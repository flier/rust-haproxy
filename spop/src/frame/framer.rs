use std::mem;

use bytes::{BufMut, BytesMut};
use futures::pin_mut;
use hexplay::HexView;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::trace;

use crate::{
    error::{Error::*, Result},
    frame::{BufExt, BufMutExt, Frame},
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
            let mut buf = {
                let mut buf = BytesMut::with_capacity(self.max_frame_size);
                buf.resize(len as usize, 0);
                r.read_exact(&mut buf).await.map_err(|_| Io)?;
                buf.freeze()
            };

            trace!(frame=%HexView::new(&buf));

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
            let mut buf = BytesMut::with_capacity(self.max_frame_size);
            buf.put_u32(0);
            buf.put_frame(frame);

            let len = (buf.len() - mem::size_of::<u32>()) as u32;
            (&mut buf[0..4]).put_u32(len);

            buf.freeze()
        };

        trace!(frame=%HexView::new(&buf));

        pin_mut!(w);

        w.write_all(&buf).await.map_err(|_| Io)?;

        Ok(buf.len())
    }
}
