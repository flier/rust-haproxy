use std::pin::Pin;
use std::task::{Context, Poll};

use derive_more::{From, Into};
use futures::Stream;
use tokio::sync::mpsc::{error::TryRecvError::*, UnboundedReceiver};

use crate::{runtime::Acker, spop::Message};

#[derive(Debug, From, Into)]
pub struct Processor(pub UnboundedReceiver<(Acker, UnboundedReceiver<Message>)>);

impl Stream for Processor {
    type Item = (Acker, Messages);

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.0.try_recv() {
            Ok((acker, receiver)) => Poll::Ready(Some((acker, Messages(receiver)))),
            Err(Empty) => Poll::Pending,
            Err(Disconnected) => Poll::Ready(None),
        }
    }
}

#[derive(Debug, From, Into)]
pub struct Messages(UnboundedReceiver<Message>);

impl Stream for Messages {
    type Item = Message;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.0.try_recv() {
            Ok(res) => Poll::Ready(Some(res)),
            Err(Empty) => Poll::Pending,
            Err(Disconnected) => Poll::Ready(None),
        }
    }
}
