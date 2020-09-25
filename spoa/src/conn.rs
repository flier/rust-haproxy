use std::mem;

use anyhow::{anyhow, bail, Context, Result};
use bytes::{Buf, BytesMut};
use tokio::{io::AsyncReadExt, net::TcpStream};
use tracing::instrument;

use crate::spop::Frame;

const MAX_FRAME_SIZE: usize = 16384;

#[derive(Debug)]
pub struct Connection {
    stream: TcpStream,
    buffer: BytesMut,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Connection {
            stream,
            buffer: BytesMut::with_capacity(MAX_FRAME_SIZE),
        }
    }

    #[instrument]
    pub async fn read_frame(&mut self) -> Result<Option<Frame>> {
        loop {
            if let Some(frame) = self.parse_frame()? {
                return Ok(Some(frame));
            }

            if 0 == self.stream.read_buf(&mut self.buffer).await? {
                if self.buffer.is_empty() {
                    return Ok(None);
                }

                bail!("connection reset by peer");
            }
        }
    }

    #[instrument]
    fn parse_frame(&mut self) -> Result<Option<Frame>> {
        if self.buffer.len() > Frame::LENGTH_SIZE {
            let frame_length = self.buffer.get_u32() as usize;

            if self.buffer.len() > Frame::LENGTH_SIZE + frame_length {
                let mut buf = self.buffer.split_to(Frame::LENGTH_SIZE + frame_length);
                let buf = buf.split_to(Frame::LENGTH_SIZE).freeze();

                return Frame::parse(&buf)
                    .map(|(frame, _)| Some(frame))
                    .map_err(|err| anyhow!("parse frame failed, {:?}", err));
            }
        }

        Ok(None)
    }
}
