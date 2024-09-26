use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufStream};
use tracing::instrument;

use crate::{
    error::{Error::Io, Result},
    frame::{Frame, Framer},
};

pub type BufCodec<T> = Codec<BufStream<T>>;

impl<T> BufCodec<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    pub fn buffered(stream: T, framer: Framer) -> Self {
        Self {
            stream: BufStream::new(stream),
            framer,
        }
    }
}

#[derive(Debug)]
pub struct Codec<T> {
    stream: T,
    framer: Framer,
}

impl<T> Codec<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(stream: T, framer: Framer) -> Self {
        Self { stream, framer }
    }

    #[instrument(skip(self), ret, err, level = "trace")]
    pub async fn read_frame(&mut self) -> Result<Frame> {
        self.framer.read_frame(&mut self.stream).await
    }

    #[instrument(skip(self), ret, err, level = "trace")]
    pub async fn write_frame(&mut self, frame: Frame) -> Result<usize> {
        let sz = self.framer.write_frame(&mut self.stream, frame).await?;

        self.stream.flush().await.map_err(|_| Io)?;

        Ok(sz)
    }
}
