use std::{collections::HashSet, sync::Arc, time::Duration};

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
    pub fn version(&mut self, version: Version) -> &mut Self {
        self.supported_versions.insert(version);
        self
    }

    pub fn fragmentation(&mut self) -> &mut Self {
        self.capabilities.insert(Capability::Fragmentation);
        self
    }

    pub fn pipelining(&mut self) -> &mut Self {
        self.capabilities.insert(Capability::Pipelining);
        self
    }

    pub fn asynchronous(&mut self) -> &mut Self {
        self.capabilities.insert(Capability::Async);
        self
    }

    pub fn capabilities<I>(&mut self, caps: I) -> &mut Self
    where
        I: IntoIterator<Item = Capability>,
    {
        self.capabilities.extend(caps);
        self
    }

    pub fn capability(&mut self, cap: Capability) -> &mut Self {
        self.capabilities.insert(cap);
        self
    }

    pub fn max_frame_size(&mut self, sz: u32) -> &mut Self {
        self.max_frame_size = Some(sz);
        self
    }

    pub fn max_process_time(&mut self, d: Duration) -> &mut Self {
        self.max_process_time = Some(d);
        self
    }

    pub fn build(self) -> Arc<Runtime> {
        Arc::new(Runtime::new(
            self.supported_versions.into_iter().collect(),
            self.capabilities.into_iter().collect(),
            self.max_frame_size.unwrap_or(MAX_FRAME_SIZE),
            self.max_process_time.unwrap_or(MAX_PROCESS_TIME),
        ))
    }
}
