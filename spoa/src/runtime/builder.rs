use std::{collections::HashSet, sync::Arc, time::Duration};

use haproxy_spop::{Action, Message};
use tower::MakeService;

use crate::{
    runtime::{Runtime, MAX_PROCESS_TIME},
    spop::{Capability, Version, MAX_FRAME_SIZE},
};

#[derive(Debug, Default)]
pub struct Builder {
    pub supported_versions: HashSet<Version>,
    pub capabilities: HashSet<Capability>,
    pub max_frame_size: Option<u32>,
    pub max_process_time: Option<Duration>,
}
impl Builder {
    pub fn new() -> Builder {
        Builder::default()
    }

    pub fn version(mut self, version: Version) -> Self {
        self.supported_versions.insert(version);
        self
    }

    pub fn fragmentation(mut self) -> Self {
        self.capabilities.insert(Capability::Fragmentation);
        self
    }

    pub fn pipelining(mut self) -> Self {
        self.capabilities.insert(Capability::Pipelining);
        self
    }

    pub fn asynchronous(mut self) -> Self {
        self.capabilities.insert(Capability::Async);
        self
    }

    pub fn capabilities<I>(mut self, caps: I) -> Self
    where
        I: IntoIterator<Item = Capability>,
    {
        self.capabilities.extend(caps);
        self
    }

    pub fn capability(mut self, cap: Capability) -> Self {
        self.capabilities.insert(cap);
        self
    }

    pub fn max_frame_size(mut self, sz: u32) -> Self {
        self.max_frame_size = Some(sz);
        self
    }

    pub fn max_process_time(mut self, d: Duration) -> Self {
        self.max_process_time = Some(d);
        self
    }

    pub fn make_service<S, T>(self, make_service: S, state: T) -> Arc<Runtime<S, T>>
    where
        S: MakeService<T, Vec<Message>, Response = Vec<Action>>,
    {
        Arc::new(Runtime::new(
            self.supported_versions.into_iter().collect(),
            self.capabilities.into_iter().collect(),
            self.max_frame_size.unwrap_or(MAX_FRAME_SIZE),
            self.max_process_time.unwrap_or(MAX_PROCESS_TIME),
            make_service,
            state,
        ))
    }
}
