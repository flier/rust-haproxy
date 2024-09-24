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

    pub fn new<S, I, K, V>(name: S, args: I) -> Self
    where
        S: Into<String>,
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<Typed>,
    {
        Message {
            name: name.into(),
            args: args
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    pub fn builder<S: Into<String>>(name: S) -> Builder {
        Builder(Message {
            name: name.into(),
            args: vec![],
        })
    }

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

#[derive(Clone, Debug)]
pub struct Builder(Message);

impl Builder {
    pub fn arg<S: Into<String>, V: Into<Typed>>(mut self, name: S, value: V) -> Self {
        self.0.args.push((name.into(), value.into()));
        self
    }

    pub fn args<I: IntoIterator<Item = (K, V)>, K: Into<String>, V: Into<Typed>>(
        mut self,
        args: I,
    ) -> Self {
        self.0
            .args
            .extend(args.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    pub fn build(self) -> Message {
        self.0
    }
}
