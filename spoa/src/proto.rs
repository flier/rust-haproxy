use std::collections::HashSet;
use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::ready;
use pin_project::pin_project;

use crate::{
    accept::Accept,
    error::{Context as _, Result},
    msgs::{Acker, Messages},
    service::MakeServiceRef,
    spop::{Capability, Version},
};

pub const MAX_FRAME_SIZE: usize = 16384;

/// A lower-level configuration of the SPOP protocol.
#[derive(Clone, Debug)]
pub struct Protocol {
    pub supported_versions: HashSet<Version>,
    pub max_frame_size: u32,
    pub capabilities: HashSet<Capability>,
}

impl Default for Protocol {
    fn default() -> Self {
        let mut supported_versions = HashSet::new();
        supported_versions.insert(Version::default());

        let mut capabilities = HashSet::new();
        capabilities.insert(Capability::Fragmentation);
        capabilities.insert(Capability::Async);
        capabilities.insert(Capability::Pipelining);

        Self {
            supported_versions,
            max_frame_size: MAX_FRAME_SIZE as u32,
            capabilities,
        }
    }
}

impl Protocol {
    pub fn supported_version(&mut self, version: Version) -> &mut Self {
        self.supported_versions.insert(version);
        self
    }

    pub fn max_frame_size(&mut self, max_frame_size: u32) -> &mut Self {
        self.max_frame_size = max_frame_size;
        self
    }

    pub fn with_capability(&mut self, capability: Capability) -> &mut Self {
        self.capabilities.insert(capability);
        self
    }

    pub fn serve<I, S>(self, incoming: I, make_service: S) -> Serve<I, S> {
        Serve {
            incoming,
            make_service,
            protocol: self,
        }
    }
}

/// A stream mapping incoming IOs to new services.
///
/// Yields `Connecting`s that are futures that should be put on a reactor.
#[must_use = "streams do nothing unless polled"]
#[pin_project]
#[derive(Debug)]
pub struct Serve<I, S> {
    #[pin]
    incoming: I,
    protocol: Protocol,
    make_service: S,
}

impl<I, S> Serve<I, S> {
    /// Spawn all incoming connections.
    pub(super) fn spawn_all(self) -> SpawnAll<I, S> {
        SpawnAll { serve: self }
    }
}

impl<I, IO, IE, S> Future for Serve<I, S>
where
    I: Accept<Conn = IO, Error = IE>,
    IE: StdError + Send + Sync + 'static,
    S: MakeServiceRef<IO, (Acker, Messages), Error = IE>,
{
    type Output = Option<Result<Handshaking<S::Future, IO>>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let me = self.project();

        match ready!(me.make_service.poll_ready_ref(cx)) {
            Ok(()) => (),
            Err(err) => {
                trace!("make_service closed");
                return Poll::Ready(Some(Err(err).context("make service")));
            }
        }

        if let Some(item) = ready!(me.incoming.poll_accept(cx)) {
            let io = item.context("accept")?;
            let future = me.make_service.make_service_ref(&io);
            Poll::Ready(Some(Ok(Handshaking {
                future,
                io: Some(io),
                protocol: me.protocol.clone(),
            })))
        } else {
            Poll::Ready(None)
        }
    }
}

pub struct Handshaking<F, IO> {
    future: F,
    io: Option<IO>,
    protocol: Protocol,
}

#[must_use = "futures do nothing unless polled"]
#[pin_project]
#[derive(Debug)]
pub struct SpawnAll<I, S> {
    #[pin]
    pub serve: Serve<I, S>,
}

impl<I, IO, IE, S> Future for SpawnAll<I, S>
where
    I: Accept<Conn = IO, Error = IE>,
    IE: StdError + Send + Sync + 'static,
    S: MakeServiceRef<IO, (Acker, Messages), Error = IE>,
{
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut me = self.project();
        loop {
            if let Some(conn) = ready!(me.serve.as_mut().poll(cx)?) {
            } else {
                return Poll::Ready(Ok(()));
            }
        }
    }
}
