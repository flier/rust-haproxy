use std::mem;
use std::net::Shutdown;

use anyhow::{Context, Result};
use bytes::{BufMut, BytesMut};
use hexplay::HexViewBuilder;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::{debug, instrument, trace};

use crate::spop::{BufMutExt, Frame, Status};

pub const MAX_FRAME_SIZE: usize = 16384;

#[derive(Debug)]
pub struct Connection {
    stream: TcpStream,
    max_frame_size: usize,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Connection {
            stream,
            max_frame_size: MAX_FRAME_SIZE,
        }
    }

    #[instrument]
    pub async fn read_frame(&mut self) -> Result<Frame> {
        let frame_length = self.stream.read_u32().await.context("read frame length")? as usize;
        if frame_length > self.max_frame_size {
            debug!("invalid frame length, {:x}", frame_length);

            return Err(Status::BadFrameSize).context("frame too large");
        }

        let mut buf = BytesMut::with_capacity(self.max_frame_size);
        buf.resize(frame_length, 0);
        self.stream
            .read_exact(&mut buf)
            .await
            .context("read frame")?;

        let buf = buf.freeze();
        trace!(len = frame_length, buf = %HexViewBuilder::new(&buf).finish(), "frame ready");

        match Frame::parse(&buf) {
            Ok((frame, _)) => {
                trace!(?frame, "frame parsed");

                Ok(frame)
            }
            Err(err) => {
                debug!(?err, "parse failed");

                Err(Status::Invalid).context("parse frame")
            }
        }
    }

    #[instrument]
    pub async fn write_frame(&mut self, frame: Frame) -> Result<usize> {
        let frame_length = frame.size();
        let mut buf = BytesMut::with_capacity(Frame::LENGTH_SIZE + frame_length);
        buf.put_u32(frame_length as u32);
        buf.put_frame(frame);

        let buf = buf.freeze();
        self.stream.write_all(&buf).await.context("write frame")?;
        trace!(buf = %HexViewBuilder::new(&buf).finish(), "frame wrote");

        Ok(Frame::LENGTH_SIZE + frame_length)
    }
}
