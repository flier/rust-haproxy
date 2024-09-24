use tokio::sync::mpsc::{self, Receiver, Sender};
use tower::{MakeService, Service};

use crate::spop::{
    Action, AsyncHandler, Error, {Frame, Message},
};

#[derive(Debug)]
pub struct Handler<H> {
    inner: H,
    processing_messages: Sender<Vec<Message>>,
    waiting_acks: Receiver<Result<Vec<Action>, Error>>,
}

#[derive(Debug)]
pub struct Processor<S> {
    service_factory: S,
    waiting_messages: Receiver<Vec<Message>>,
    sending_acks: Sender<Result<Vec<Action>, Error>>,
}

pub fn simple<H, S>(handler: H, service_factory: S) -> (Handler<H>, Processor<S>)
where
    S: MakeService<(), Vec<Message>, Response = Vec<Action>, Error = Error> + Send + 'static,
    S::Service: Send,
    S::Future: Send,
    <S::Service as Service<Vec<Message>>>::Future: Send,
{
    let (processing_messages, waiting_messages) = mpsc::channel(1);
    let (sending_acks, waiting_acks) = mpsc::channel(1);

    let handler = Handler {
        inner: handler,
        processing_messages,
        waiting_acks,
    };

    let processor = Processor {
        service_factory,
        waiting_messages,
        sending_acks,
    };

    (handler, processor)
}

impl<H> AsyncHandler<Option<Vec<Action>>> for Handler<H>
where
    H: AsyncHandler<Option<Vec<Message>>, Error = Error>,
{
    type Error = Error;

    async fn handle_frame(&mut self, frame: Frame) -> Result<Option<Vec<Action>>, Self::Error> {
        if let Some(msgs) = self.inner.handle_frame(frame).await? {
            self.processing_messages
                .send(msgs)
                .await
                .map_err(|_| Error::Io)?;

            self.waiting_acks.recv().await.transpose()
        } else {
            Ok(None)
        }
    }
}

impl<S> Processor<S>
where
    S: MakeService<(), Vec<Message>, Response = Vec<Action>, Error = Error> + Send + 'static,
    S::Service: Send,
    S::Future: Send,
    <S::Service as Service<Vec<Message>>>::Future: Send,
{
    pub async fn serve(&mut self) -> Result<(), Error> {
        while let Some(msgs) = self.waiting_messages.recv().await {
            let mut svc = self
                .service_factory
                .make_service(())
                .await
                .map_err(|_| Error::Io)?;

            let actions = svc.call(msgs).await;

            self.sending_acks
                .send(actions)
                .await
                .map_err(|_| Error::Io)?;
        }

        Ok::<_, Error>(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {}
}
