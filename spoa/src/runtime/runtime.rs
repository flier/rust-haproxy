use std::error::Error as StdError;
use std::time::Duration;

use tokio::sync::{mpsc::unbounded_channel, RwLock};
use tower::MakeService;

use crate::{
    error::{Context, Result},
    runtime::{Dispatcher, Processor},
    spop::{Capability, Version},
};

#[derive(Debug)]
pub struct ServiceMaker<S, T> {
    maker: S,
    state: T,
}

impl<S, T> ServiceMaker<S, T> {
    pub async fn make<REQ>(&mut self) -> Result<S::Service>
    where
        S: MakeService<T, REQ>,
        S::MakeError: StdError + Send + Sync + 'static,
        T: Clone,
    {
        self.maker
            .make_service(self.state.clone())
            .await
            .context("make service")
    }
}

#[derive(Debug)]
pub struct Runtime<S, T> {
    pub dispatcher: Dispatcher,
    pub processor: Processor,
    pub supported_versions: Vec<Version>,
    pub capabilities: Vec<Capability>,
    pub max_frame_size: u32,
    pub max_process_time: Duration,
    pub service_maker: RwLock<ServiceMaker<S, T>>,
}

pub const MAX_PROCESS_TIME: Duration = Duration::from_secs(15);

impl<S, T> Runtime<S, T> {
    pub fn new(
        supported_versions: Vec<Version>,
        capabilities: Vec<Capability>,
        max_frame_size: u32,
        max_process_time: Duration,
        make_service: S,
        make_state: T,
    ) -> Self {
        let (sender, receiver) = unbounded_channel();

        Runtime {
            dispatcher: Dispatcher::new(sender),
            processor: Processor(receiver),
            supported_versions,
            capabilities,
            max_frame_size,
            max_process_time,
            service_maker: RwLock::new(ServiceMaker {
                maker: make_service,
                state: make_state,
            }),
        }
    }
}
