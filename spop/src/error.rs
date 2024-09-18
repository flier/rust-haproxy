use std::result::Result as StdResult;

use thiserror::Error;

pub type Result<T> = StdResult<T, Error>;

/// Errors triggered by SPOE applet
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Error)]
pub enum Error {
    /// normal
    #[error("normal")]
    Normal,
    /// I/O error
    #[error("I/O error")]
    Io,
    /// a timeout occurred
    #[error("a timeout occurred")]
    Timeout,
    /// frame is too big
    #[error("frame is too big")]
    TooBig,
    /// invalid frame received
    #[error("invalid frame received")]
    Invalid,
    /// version value not found
    #[error("version value not found")]
    NoVersion,
    /// max-frame-size value not found
    #[error("max-frame-size value not found")]
    NoFrameSize,
    /// capabilities value not found
    #[error("capabilities value not found")]
    NoCapabilities,
    /// unsupported version
    #[error("unsupported version")]
    BadVersion,
    /// max-frame-size too big or too small
    #[error("max-frame-size too big or too small")]
    BadFrameSize,
    /// fragmentation not supported
    #[error("fragmentation not supported")]
    FragmentNotSupported,
    /// invalid interlaced frames
    #[error("invalid interlaced frames")]
    InterlacedFrames,
    /// frame-id not found
    #[error("frame-id not found")]
    FrameIdNotFound,
    /// resource allocation error
    #[error("resource allocation error")]
    ResourceAllocErr,
    /// an unknown error occurred
    #[error("an unknown error occurred")]
    Unknown = 99,
}
