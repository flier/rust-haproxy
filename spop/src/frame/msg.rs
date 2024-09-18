use std::mem;

use crate::{data::Value, Typed};

/// The SPOE message with the name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// The name of the message.
    pub name: String,
    /// The arguments of the message.
    pub args: Vec<(String, Typed)>,
}

impl Message {
    const NB_ARGS_SIZE: usize = mem::size_of::<u8>();

    /// Returns the size of the message
    pub(crate) fn size(&self) -> usize {
        self.name.size()
            + Self::NB_ARGS_SIZE
            + self
                .args
                .iter()
                .map(|(k, v)| k.size() + v.size())
                .sum::<usize>()
    }
}
