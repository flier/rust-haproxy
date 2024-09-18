use parse_display::{Display, FromStr};

/// The SPOP version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Display, FromStr)]
#[display("{major}.{minor}")]
pub struct Version {
    pub major: u8,
    pub minor: u8,
}

impl Default for Version {
    fn default() -> Self {
        Version::V2_0
    }
}

impl Version {
    /// The SPOP versions supported by HAProxy.
    pub const SUPPORTED: &[Version] = &[Self::V2_0];
    /// The SPOP 2.0 version.
    pub const V2_0: Version = Version { major: 2, minor: 0 };

    /// Create a new SPOP version.
    pub const fn new(major: u8, minor: u8) -> Self {
        Version { major, minor }
    }
}
