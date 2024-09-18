//! The data types

mod buf;
mod ty;
mod typed;
mod value;
pub mod varint;

pub use self::buf::{BufExt, BufMutExt};
pub use self::ty::{Flags, Type};
pub use self::typed::Typed;
pub use self::value::{KeyValue, Value};
