use std::net::TcpListener as StdTcpListener;

use derive_more::{Deref, From, Into};
use tokio::net::{TcpListener, ToSocketAddrs};

use crate::error::Result;

#[derive(Debug, Deref, From, Into)]
pub struct Incoming {
    listener: TcpListener,
}

impl Incoming {
    pub async fn new<A: ToSocketAddrs>(addr: A) -> Result<Self> {
        let listener = TcpListener::bind(addr).await?;

        Ok(Incoming { listener })
    }

    pub fn from_std(std_listener: StdTcpListener) -> Result<Self> {
        let listener = TcpListener::from_std(std_listener)?;

        Ok(Incoming { listener })
    }
}
