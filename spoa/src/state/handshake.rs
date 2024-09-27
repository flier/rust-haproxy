use std::cmp;
use std::collections::HashSet;

use tracing::instrument;

use crate::{
    error::Result,
    spop::{AgentHello, Capability, Error::NoVersion, HaproxyHello, Version},
};

#[instrument(ret, err, level = "trace")]
pub fn negotiate(
    mut supported_versions: Vec<Version>,
    max_frame_size: u32,
    capabilities: Vec<Capability>,
    mut hello: HaproxyHello,
) -> Result<Negotiated> {
    hello.supported_versions.sort();
    supported_versions.sort();

    let version = hello
        .supported_versions
        .into_iter()
        .rev()
        .find(|version| supported_versions.iter().rev().any(|v| v == version))
        .ok_or(NoVersion)?;
    let max_frame_size = cmp::min(hello.max_frame_size, max_frame_size);
    let capabilities = hello
        .capabilities
        .into_iter()
        .collect::<HashSet<_>>()
        .intersection(&capabilities.into_iter().collect::<HashSet<_>>())
        .cloned()
        .collect::<Vec<_>>();

    Ok(Negotiated {
        version,
        max_frame_size,
        capabilities: capabilities.clone(),
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Negotiated {
    pub version: Version,
    pub max_frame_size: u32,
    pub capabilities: Vec<Capability>,
}

impl Negotiated {
    pub fn agent_hello(&self) -> AgentHello {
        AgentHello {
            version: self.version,
            max_frame_size: self.max_frame_size,
            capabilities: self.capabilities.clone(),
        }
    }
}
