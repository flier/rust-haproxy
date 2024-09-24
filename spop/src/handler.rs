use std::convert::{Infallible, TryFrom};

use tower::{service_fn, MakeService};

use crate::{frame::Frame, Action, AgentAck, Disconnect, Error, HaproxyNotify, Message};

#[allow(async_fn_in_trait)]
pub trait AsyncHandler<T> {
    type Error;

    async fn handle_frame(&mut self, frame: Frame) -> Result<T, Self::Error>;
}

pub fn notify_handler(
) -> impl MakeService<(), Frame, Response = Option<Vec<Message>>, Error = Error, MakeError = Infallible>
{
    service_fn(|_| async {
        Ok::<_, Infallible>(service_fn(|frame: Frame| async {
            match frame {
                Frame::HaproxyNotify(HaproxyNotify { messages, .. }) => Ok(Some(messages)),
                Frame::HaproxyDisconnect(Disconnect { status_code, .. }) => {
                    Err(Error::try_from(status_code).unwrap_or(Error::Unknown))
                }
                _ => Err(Error::Invalid),
            }
        }))
    })
}

pub fn ack_handler(
) -> impl MakeService<(), Frame, Response = Option<Vec<Action>>, Error = Error, MakeError = Infallible>
{
    service_fn(|_| async {
        Ok::<_, Infallible>(service_fn(|frame: Frame| async {
            match frame {
                Frame::AgentAck(AgentAck { actions, .. }) => Ok(Some(actions)),
                Frame::AgentDisconnect(Disconnect { status_code, .. }) => {
                    Err(Error::try_from(status_code).unwrap_or(Error::Unknown))
                }
                _ => Err(Error::Invalid),
            }
        }))
    })
}

#[cfg(test)]
mod tests {
    use tower::Service;

    use crate::{Action, Error, Frame, Message, Scope};

    use super::*;

    #[tokio::test]
    async fn test_message_handler() -> Result<(), Error> {
        let m1 = Message::new("foobar", [("foo", 123), ("bar", 456)]);
        let cases = [
            (Frame::notify(123, 456, [m1.clone()]), Ok(Some(vec![m1]))),
            (
                Frame::haproxy_disconnect(Error::Io, "some reason"),
                Err(Error::Io),
            ),
            (
                Frame::agent_disconnect(Error::Io, "some reason"),
                Err(Error::Invalid),
            ),
        ];

        let mut h = notify_handler();

        for (f, res) in cases {
            let mut svc = h.make_service(()).await.unwrap();

            assert_eq!(svc.call(f).await, res);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_action_handler() {
        let a1 = Action::set_var(Scope::Request, "foo", "bar");
        let cases = [
            (Frame::ack(123, 456, [a1.clone()]), Ok(Some(vec![a1]))),
            (
                Frame::agent_disconnect(Error::Io, "some reason"),
                Err(Error::Io),
            ),
            (
                Frame::haproxy_disconnect(Error::Io, "some reason"),
                Err(Error::Invalid),
            ),
        ];

        let mut h = ack_handler();

        for (f, res) in cases {
            let mut svc = h.make_service(()).await.unwrap();

            assert_eq!(svc.call(f).await, res);
        }
    }
}
