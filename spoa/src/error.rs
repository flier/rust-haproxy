use std::{
    error::Error as StdError,
    fmt::{Debug, Display},
    result::Result as StdResult,
};

use thiserror::Error;

use crate::{
    spop::{Error as Status, Message},
    Acker,
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
        source: Box<dyn StdError>,
        context: Box<dyn Reason>,
    },
}

unsafe impl Send for Error {}
unsafe impl Sync for Error {}

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

pub trait Reason: Display + Debug + Send + Sync + 'static {}

impl Reason for &'static str {}

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
    E: StdError + 'static,
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
