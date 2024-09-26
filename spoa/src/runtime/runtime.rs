use tokio::sync::mpsc::unbounded_channel;

use crate::runtime::{Dispatcher, Processor};

#[derive(Debug)]
pub struct Runtime {
    pub dispatcher: Dispatcher,
    pub processor: Processor,
}

impl Default for Runtime {
    fn default() -> Self {
        let (sender, receiver) = unbounded_channel();

        Runtime {
            dispatcher: Dispatcher::new(sender),
            processor: Processor(receiver),
        }
    }
}
