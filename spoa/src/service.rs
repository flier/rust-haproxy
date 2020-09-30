use std::error::Error as StdError;
use std::fmt;
use std::future::Future;
use std::task::{Context, Poll};

use tower_service::Service;

use crate::spop::Action;

pub trait MakeServiceRef<Target, Request> {
    type Response: IntoIterator<Item = Action>;
    type Error: Into<Box<dyn StdError + Send + Sync>>;
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;
    type Future: Future<Output = Result<Self::Service, Self::Error>>;

    fn poll_ready_ref(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;

    fn make_service_ref(&mut self, target: &Target) -> Self::Future;
}

impl<T, Target, Request, S, R, E, F> MakeServiceRef<Target, Request> for T
where
    T: for<'a> Service<&'a Target, Response = S, Error = E, Future = F>,
    S: Service<Request, Response = R, Error = E>,
    E: Into<Box<dyn StdError + Send + Sync>>,
    R: IntoIterator<Item = Action>,
    F: Future<Output = Result<S, E>>,
{
    type Response = R;
    type Error = E;
    type Service = S;
    type Future = F;

    fn poll_ready_ref(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_ready(cx)
    }

    fn make_service_ref(&mut self, target: &Target) -> Self::Future {
        self.call(target)
    }
}

/// Create a `MakeService` from a function.
pub fn make_service_fn<F, Target, Ret>(f: F) -> MakeServiceFn<F>
where
    F: FnMut(&Target) -> Ret,
    Ret: Future,
{
    MakeServiceFn(f)
}

/// `MakeService` returned from [`make_service_fn`]
#[derive(Clone, Copy)]
pub struct MakeServiceFn<F>(F);

impl<'t, F, Ret, Target, S, E> Service<&'t Target> for MakeServiceFn<F>
where
    F: FnMut(&Target) -> Ret,
    Ret: Future<Output = Result<S, E>>,
    E: Into<Box<dyn StdError + Send + Sync>>,
{
    type Error = E;
    type Response = S;
    type Future = Ret;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, target: &'t Target) -> Self::Future {
        self.0(target)
    }
}

impl<F> fmt::Debug for MakeServiceFn<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MakeServiceFn").finish()
    }
}
