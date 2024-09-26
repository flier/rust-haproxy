use std::{collections::HashSet, error::Error as StdError, sync::Arc};

use tokio::io::{AsyncRead, AsyncWrite};
use tower::Service;

use crate::{
    conn::Connection,
    spop::{Capability, Frame, MAX_FRAME_SIZE},
    Runtime,
};

/// A configuration builder for SPOA agent connections.
#[derive(Clone, Debug)]
pub struct Builder {
    max_frame_size: Option<usize>,
    caps: HashSet<Capability>,
}

impl Builder {
    /// Create a new connection builder.
    pub fn new() -> Self {
        Self {
            max_frame_size: None,
            caps: HashSet::from([Capability::Pipelining]),
        }
    }

    pub fn max_frame_size(&mut self, sz: usize) -> &mut Self {
        self.max_frame_size = Some(sz);
        self
    }

    /// Enables or disables async frame.
    #[deprecated]
    pub fn asynchronous(&mut self, enable: bool) -> &mut Self {
        if enable {
            self.caps.insert(Capability::Async);
        } else {
            self.caps.remove(&Capability::Async);
        }

        self
    }

    /// Enables or disables frame fragmentation.
    #[deprecated]
    pub fn fragmentation(&mut self, enable: bool) -> &mut Self {
        if enable {
            self.caps.insert(Capability::Fragmentation);
        } else {
            self.caps.remove(&Capability::Fragmentation);
        }

        self
    }

    /// Enables or disables frame pipelining.
    pub fn pipelining(&mut self, enable: bool) -> &mut Self {
        if enable {
            self.caps.insert(Capability::Pipelining);
        } else {
            self.caps.remove(&Capability::Pipelining);
        }

        self
    }

    /// Bind a connection together with a Service.
    pub fn build<IO, S>(&self, io: IO, service: S) -> Connection<IO, S>
    where
        IO: AsyncRead + AsyncWrite + Unpin,
        S: Service<Frame>,
        S::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        Connection::new(
            Arc::new(Runtime::default()),
            io,
            self.max_frame_size.unwrap_or(MAX_FRAME_SIZE),
            service,
        )
    }
}
