use std::{mem, pin::Pin};

use bytes::{BufMut, Bytes, BytesMut};
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
            let mut buf = read_frame(r, self.max_frame_size, len).await?;

            trace!(buf=%HexView::new(&buf));

            buf.get_frame().map_err(|_| Invalid)
        } else {
            Err(BadFrameSize)
        }
    }

    pub async fn write_frame<W>(&self, w: W, frame: Frame) -> Result<usize>
    where
        W: AsyncWrite + Sized,
    {
        let buf = write_frame(BytesMut::with_capacity(self.max_frame_size), frame);

        trace!(buf=%HexView::new(&buf[4..]));

        pin_mut!(w);

        w.write_all(&buf).await.map_err(|_| Io)?;

        Ok(buf.len())
    }
}

async fn read_frame<R>(mut r: Pin<&mut R>, max_frame_size: usize, len: usize) -> Result<Bytes>
where
    R: AsyncRead + Sized,
{
    let mut buf = BytesMut::with_capacity(max_frame_size);
    buf.resize(len, 0);

    r.read_exact(&mut buf).await.map_err(|_| Io)?;

    Ok(buf.freeze())
}

fn write_frame(mut buf: BytesMut, frame: Frame) -> Bytes {
    buf.put_u32(0);
    buf.put_frame(frame);

    let len = (buf.len() - mem::size_of::<u32>()) as u32;

    (&mut buf[0..4]).put_u32(len);

    buf.freeze()
}
