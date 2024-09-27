use std::time::Duration;

use tokio::sync::mpsc::unbounded_channel;

use crate::runtime::{Dispatcher, Processor};

#[derive(Debug)]
pub struct Runtime {
    pub dispatcher: Dispatcher,
    pub processor: Processor,
    pub max_process_time: Duration,
}

const MAX_PROCESS_TIME: Duration = Duration::from_secs(15);

impl Default for Runtime {
    fn default() -> Self {
        let (sender, receiver) = unbounded_channel();

        Runtime {
            dispatcher: Dispatcher::new(sender),
            processor: Processor(receiver),
            max_process_time: MAX_PROCESS_TIME,
        }
    }
}
