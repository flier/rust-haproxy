use bytes::{BufMut, BytesMut};
use hexplay::HexViewBuilder;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::instrument;

use crate::{
    error::{Context, Result},
    proto::MAX_FRAME_SIZE,
    spop::{parse_frame, put_frame, Error as Status, Frame},
};

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

    #[instrument(skip_all, ret, err, level = "trace")]
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

        Ok(parse_frame(buf)?)
    }

    #[instrument]
    pub async fn write_frame(&mut self, frame: Frame) -> Result<usize> {
        let frame_length = frame.size();
        let mut buf = BytesMut::with_capacity(Frame::LENGTH_SIZE + frame_length);
        buf.put_u32(frame_length as u32);
        put_frame(&mut buf, frame);

        let buf = buf.freeze();
        self.stream.write_all(&buf).await.context("write frame")?;
        trace!(buf = %HexViewBuilder::new(&buf).finish(), "frame wrote");

        Ok(Frame::LENGTH_SIZE + frame_length)
    }
}
