use tokio::net::TcpStream;
use tracing::instrument;

use crate::{
    error::Result,
    proto::MAX_FRAME_SIZE,
    spop::{Error as Status, Frame, Framer},
};

#[derive(Debug)]
pub struct Connection {
    stream: TcpStream,
    framer: Framer,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Connection {
            stream,
            framer: Framer::new(MAX_FRAME_SIZE),
        }
    }

    pub async fn disconnect<S: Into<String>>(&mut self, status: Status, msg: S) -> Result<()> {
        let disconnect = Frame::agent_disconnect(status, msg);
        self.write_frame(disconnect).await?;
        Ok(())
    }

    #[instrument(skip_all, ret, err, level = "trace")]
    pub async fn read_frame(&mut self) -> Result<Frame> {
        Ok(self.framer.read_frame(&mut self.stream).await?)
    }

    #[instrument(skip(self), ret, err, level = "trace")]
    pub async fn write_frame(&mut self, frame: Frame) -> Result<usize> {
        Ok(self.framer.write_frame(&mut self.stream, frame).await?)
    }
}
