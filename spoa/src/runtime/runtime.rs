use std::time::Duration;

use haproxy_spop::MAX_FRAME_SIZE;
use tokio::sync::mpsc::unbounded_channel;

use crate::{
    runtime::{Builder, Dispatcher, Processor},
    spop::{Capability, Version},
};

#[derive(Debug)]
pub struct Runtime {
    pub dispatcher: Dispatcher,
    pub processor: Processor,
    pub supported_versions: Vec<Version>,
    pub capabilities: Vec<Capability>,
    pub max_frame_size: u32,
    pub max_process_time: Duration,
}

pub const MAX_PROCESS_TIME: Duration = Duration::from_secs(15);

impl Default for Runtime {
    fn default() -> Self {
        Runtime::new(
            vec![Version::V2_0],
            vec![
                Capability::Fragmentation,
                Capability::Pipelining,
                Capability::Async,
            ],
            MAX_FRAME_SIZE,
            MAX_PROCESS_TIME,
        )
    }
}

impl Runtime {
    pub fn new(
        supported_versions: Vec<Version>,
        capabilities: Vec<Capability>,
        max_frame_size: u32,
        max_process_time: Duration,
    ) -> Self {
        let (sender, receiver) = unbounded_channel();

        Runtime {
            dispatcher: Dispatcher::new(sender),
            processor: Processor(receiver),
            supported_versions,
            capabilities,
            max_frame_size,
            max_process_time,
        }
    }

    pub fn builder() -> Builder {
        Builder::default()
    }
}
