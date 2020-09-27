use std::cmp;
use std::collections::HashSet;

use anyhow::Result;

use crate::conn::MAX_FRAME_SIZE;
use crate::spop::{agent, haproxy, Capability, Status, Version};

#[derive(Clone, Debug, PartialEq)]
pub struct Handshaking {
    pub supported_versions: Vec<Version>,
    pub max_frame_size: u32,
    pub capabilities: Vec<Capability>,
}

impl Default for Handshaking {
    fn default() -> Self {
        Handshaking {
            supported_versions: vec![Version::default()],
            max_frame_size: MAX_FRAME_SIZE as u32,
            capabilities: vec![
                Capability::Fragmentation,
                Capability::Async,
                Capability::Pipelining,
            ],
        }
    }
}

impl Handshaking {
    pub fn handshake(mut self, mut hello: haproxy::Hello) -> Result<Handshaked> {
        hello.supported_versions.sort();
        self.supported_versions.sort();

        let version = hello
            .supported_versions
            .into_iter()
            .rev()
            .find(|version| self.supported_versions.iter().any(|v| v == version))
            .ok_or_else(|| Status::NoVersion)?;
        let max_frame_size = cmp::min(hello.max_frame_size, self.max_frame_size);
        let capabilities = hello
            .capabilities
            .into_iter()
            .collect::<HashSet<_>>()
            .intersection(&self.capabilities.into_iter().collect::<HashSet<_>>())
            .cloned()
            .collect::<Vec<_>>();

        Ok(Handshaked {
            version,
            max_frame_size,
            capabilities: capabilities.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Handshaked {
    pub version: Version,
    pub max_frame_size: u32,
    pub capabilities: Vec<Capability>,
}

impl Handshaked {
    pub fn agent_hello(&self) -> agent::Hello {
        agent::Hello {
            version: self.version,
            max_frame_size: self.max_frame_size,
            capabilities: self.capabilities.clone(),
        }
    }
}
