use std::mem;

use bitflags::bitflags;

use crate::data::varint;

/// The stream identifier
pub type StreamId = u64;
/// The frame identifier inside the stream
pub type FrameId = u64;

bitflags! {
    /// Flags set on the SPOE frame
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Flags: u32 {
        /// Indicates that this is the final payload fragment.
        const FIN = 0x00000001;
        /// Indicates that the processing of the current frame must be cancelled.
        const ABORT = 0x00000002;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub flags: Flags,
    pub stream_id: StreamId,
    pub frame_id: FrameId,
}

impl Default for Metadata {
    fn default() -> Self {
        Metadata {
            flags: Flags::FIN,
            stream_id: 0,
            frame_id: 0,
        }
    }
}

impl Metadata {
    /// Indicates that this is the final payload fragment
    pub const fn is_final(&self) -> bool {
        self.flags.contains(Flags::FIN)
    }

    /// Indicates that this is a payload fragment.
    pub const fn fragmented(&self) -> bool {
        !self.is_final()
    }

    /// Indicates that the processing of the current frame must be cancelled.
    pub const fn aborted(&self) -> bool {
        self.flags.contains(Flags::ABORT)
    }

    pub const fn size(&self) -> usize {
        mem::size_of::<Flags>() + varint::size_of(self.stream_id) + varint::size_of(self.frame_id)
    }
}
