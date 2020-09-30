use std::error::Error as StdError;
use std::future::Future;
use std::net::TcpListener as StdTcpListener;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::Result;
use pin_project::pin_project;
use tokio::net::ToSocketAddrs;

use crate::{
    accept::Accept,
    msgs::{Acker, Messages},
    proto::{Protocol, SpawnAll},
    service::MakeServiceRef,
    spop::{Capability, Version},
    tcp::Incoming,
};

#[pin_project]
pub struct Server<I, S> {
    #[pin]
    spawn_all: SpawnAll<I, S>,
}

/// A builder for a [`Agent`](Agent).
#[derive(Debug)]
pub struct Builder<I> {
    incoming: I,
    protocol: Protocol,
}

impl<I> Server<I, ()> {
    /// Starts a [`Builder`](Builder) with the provided incoming stream.
    pub fn builder(incoming: I) -> Builder<I> {
        Builder {
            incoming,
            protocol: Protocol::default(),
        }
    }
}

impl Server<Incoming, ()> {
    /// Binds to the provided address, and returns a [`Builder`](Builder).
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> Result<Builder<Incoming>> {
        Incoming::new(addr).await.map(Server::builder)
    }

    /// Create a new instance from a `std::net::TcpListener` instance.
    pub fn from_tcp(listener: StdTcpListener) -> Result<Builder<Incoming>> {
        Incoming::from_std(listener).map(Server::builder)
    }
}

impl<I, IO, IE, S> Future for Server<I, S>
where
    I: Accept<Conn = IO, Error = IE>,
    IE: StdError + Send + Sync + 'static,
    S: MakeServiceRef<IO, (Acker, Messages), Error = IE>,
{
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project().spawn_all.poll(cx)
    }
}

impl<I> Builder<I> {
    pub fn supported_version(mut self, version: Version) -> Self {
        self.protocol.supported_version(version);
        self
    }

    pub fn max_frame_size(mut self, max_frame_size: u32) -> Self {
        self.protocol.max_frame_size(max_frame_size);
        self
    }

    pub fn with_capability(mut self, capability: Capability) -> Self {
        self.protocol.with_capability(capability);
        self
    }

    pub fn serve<S>(self, new_service: S) -> Server<I, S> {
        let serve = self.protocol.serve(self.incoming, new_service);

        Server {
            spawn_all: serve.spawn_all(),
        }
    }
}
