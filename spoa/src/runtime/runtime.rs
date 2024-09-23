use tokio::sync::mpsc::unbounded_channel;

use crate::{
    proto::MAX_FRAME_SIZE,
    runtime::{Dispatcher, Processor},
};

#[derive(Debug)]
pub struct Runtime {
    pub max_frame_size: usize,
    pub dispatcher: Dispatcher,
    pub processor: Processor,
}

impl Default for Runtime {
    fn default() -> Self {
        let (sender, receiver) = unbounded_channel();

        Runtime {
            max_frame_size: MAX_FRAME_SIZE,
            dispatcher: Dispatcher::new(sender),
            processor: Processor(receiver),
        }
    }
}
