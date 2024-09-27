pub mod agent;
mod codec;
mod decode;
mod disconnect;
mod encode;
mod fragment;
mod framer;
mod frames;
pub mod haproxy;
mod kv;
mod metadata;
mod msg;
mod ty;

pub use self::codec::{BufCodec, Codec};
pub use self::decode::BufExt;
pub use self::disconnect::Disconnect;
pub use self::encode::BufMutExt;
pub use self::fragment::Reassembly;
pub use self::framer::Framer;
pub use self::frames::Frame;
pub use self::metadata::{Flags, FrameId, Metadata, StreamId};
pub use self::msg::Message;
pub use self::ty::Type;

pub const MAX_FRAME_SIZE: usize = 16384;
