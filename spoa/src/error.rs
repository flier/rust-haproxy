use std::{
    error::Error as StdError,
    fmt::{Debug, Display},
    result::Result as StdResult,
};

use thiserror::Error;

use crate::{
    runtime::Acker,
    spop::{Disconnect, Error as Status, Message},
};

pub type Result<T> = StdResult<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("closed")]
    Closed,

    #[error(transparent)]
    Status(#[from] crate::spop::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Send(
        #[from]
        tokio::sync::mpsc::error::SendError<(
            Acker,
            tokio::sync::mpsc::UnboundedReceiver<Message>,
        )>,
    ),

    #[error("{context}, {source}")]
    Context {
        #[source]
        source: Box<dyn StdError + Send + Sync>,
        context: Box<dyn Reason>,
    },
}

impl Error {
    pub fn status(&self) -> Option<Status> {
        match self {
            Error::Status(status) => Some(*status),
            Error::Context { source, .. } => {
                if let Some(err) = source.downcast_ref::<Error>() {
                    err.status()
                } else {
                    source.downcast_ref::<Status>().cloned()
                }
            }
            _ => None,
        }
    }
}

impl From<Error> for Disconnect {
    fn from(err: Error) -> Self {
        match err {
            Error::Status(status) => Disconnect::new(status, status.to_string()),
            Error::Context {
                ref source,
                ref context,
            } => {
                if let Some(status) = source.downcast_ref::<Error>().and_then(|err| err.status()) {
                    Disconnect::new(status, context.to_string())
                } else if let Some(status) = source.downcast_ref::<Status>() {
                    Disconnect::new(*status, context.to_string())
                } else {
                    Disconnect::new(Status::Unknown, err.to_string())
                }
            }
            _ => Disconnect::new(Status::Unknown, err.to_string()),
        }
    }
}

pub trait Reason: Display + Debug + Send + Sync + 'static {}

impl Reason for &'static str {}
impl Reason for String {}

pub trait Context<T, E> {
    fn context<C>(self, context: C) -> StdResult<T, Error>
    where
        C: Reason;

    fn with_context<C, F>(self, f: F) -> StdResult<T, Error>
    where
        C: Reason,
        F: FnOnce() -> C;
}

impl<T, E> Context<T, E> for StdResult<T, E>
where
    E: StdError + Send + Sync + 'static,
{
    fn context<C>(self, reason: C) -> StdResult<T, Error>
    where
        C: Reason,
    {
        match self {
            Ok(res) => Ok(res),
            Err(err) => Err(Error::Context {
                source: Box::new(err),
                context: Box::new(reason),
            }),
        }
    }

    fn with_context<C, F>(self, f: F) -> StdResult<T, Error>
    where
        C: Reason,
        F: FnOnce() -> C,
    {
        match self {
            Ok(res) => Ok(res),
            Err(err) => Err(Error::Context {
                source: Box::new(err),
                context: Box::new(f()),
            }),
        }
    }
}
