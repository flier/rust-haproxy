use thiserror::Error;

const SPOE_FRM_ERR_NONE: u32 = 0;
const SPOE_FRM_ERR_IO: u32 = 1;
const SPOE_FRM_ERR_TOUT: u32 = 2;
const SPOE_FRM_ERR_TOO_BIG: u32 = 3;
const SPOE_FRM_ERR_INVALID: u32 = 4;
const SPOE_FRM_ERR_NO_VSN: u32 = 5;
const SPOE_FRM_ERR_NO_FRAME_SIZE: u32 = 6;
const SPOE_FRM_ERR_NO_CAP: u32 = 7;
const SPOE_FRM_ERR_BAD_VSN: u32 = 8;
const SPOE_FRM_ERR_BAD_FRAME_SIZE: u32 = 9;
const SPOE_FRM_ERR_FRAG_NOT_SUPPORTED: u32 = 10;
const SPOE_FRM_ERR_INTERLACED_FRAMES: u32 = 11;
const SPOE_FRM_ERR_FRAMEID_NOTFOUND: u32 = 12;
const SPOE_FRM_ERR_RES: u32 = 13;
const SPOE_FRM_ERR_UNKNOWN: u32 = 99;

/// Errors triggered by SPOE applet
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Error)]
pub enum Status {
    #[error("normal")]
    None = SPOE_FRM_ERR_NONE,
    #[error("I/O error")]
    Io = SPOE_FRM_ERR_IO,
    #[error("a timeout occurred")]
    Timeout = SPOE_FRM_ERR_TOUT,
    #[error("frame is too big")]
    TooBig = SPOE_FRM_ERR_TOO_BIG,
    #[error("invalid frame received")]
    Invalid = SPOE_FRM_ERR_INVALID,
    #[error("version value not found")]
    NoVersion = SPOE_FRM_ERR_NO_VSN,
    #[error("max-frame-size value not found")]
    NoFrameSize = SPOE_FRM_ERR_NO_FRAME_SIZE,
    #[error("capabilities value not found")]
    NoCapabilities = SPOE_FRM_ERR_NO_CAP,
    #[error("unsupported version")]
    BadVersion = SPOE_FRM_ERR_BAD_VSN,
    #[error("max-frame-size too big or too small")]
    BadFrameSize = SPOE_FRM_ERR_BAD_FRAME_SIZE,
    #[error("fragmentation not supported")]
    FragmentNotSupported = SPOE_FRM_ERR_FRAG_NOT_SUPPORTED,
    #[error("invalid interlaced frames")]
    InterlacedFrames = SPOE_FRM_ERR_INTERLACED_FRAMES,
    #[error("frame-id not found")]
    FrameIdNotFound = SPOE_FRM_ERR_FRAMEID_NOTFOUND,
    #[error("resource allocation error")]
    ResourceAllocErr = SPOE_FRM_ERR_RES,
    #[error("an unknown error occurred")]
    Unknonw = SPOE_FRM_ERR_UNKNOWN,
}
