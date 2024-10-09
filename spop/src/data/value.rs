use std::borrow::Cow;

use derive_more::Into;

/// The Key-Value pair can be used in a KV-list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyValue<'a, T>(pub(crate) Cow<'a, str>, pub(crate) T);

impl<T> From<(&'static str, T)> for KeyValue<'static, T> {
    fn from((key, value): (&'static str, T)) -> Self {
        KeyValue(key.into(), value)
    }
}

impl<T> From<(String, T)> for KeyValue<'_, T> {
    fn from((key, value): (String, T)) -> Self {
        KeyValue(key.into(), value)
    }
}
