use core::fmt;
use std::{array::IntoIter, borrow::Cow, slice::Iter};

use crate::{data::KeyValue, Capability, Typed, Version};

/* Predefined key used in HELLO/DISCONNECT frames */
pub const SUPPORTED_VERSIONS_KEY: &str = "supported-versions";
pub const VERSION_KEY: &str = "version";
pub const MAX_FRAME_SIZE_KEY: &str = "max-frame-size";
pub const CAPABILITIES_KEY: &str = "capabilities";
pub const ENGINE_ID_KEY: &str = "engine-id";
pub const HEALTHCHECK_KEY: &str = "healthcheck";
pub const STATUS_CODE_KEY: &str = "status-code";
pub const MSG_KEY: &str = "message";

pub struct Punctuated<I>(I, &'static str);

fn punctuated<I, T>(i: I) -> Punctuated<I::IntoIter>
where
    I: IntoIterator<Item = T>,
{
    Punctuated(i.into_iter(), ",")
}

impl<I, T> From<Punctuated<I>> for Typed
where
    I: IntoIterator<Item = T>,
    T: fmt::Display,
{
    fn from(Punctuated(items, sep): Punctuated<I>) -> Self {
        Typed::String(
            items
                .into_iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(sep),
        )
    }
}

pub fn supported_versions(versions: &[Version]) -> KeyValue<Punctuated<Iter<Version>>> {
    KeyValue(Cow::Borrowed(SUPPORTED_VERSIONS_KEY), punctuated(versions))
}

pub fn version(version: Version) -> KeyValue<'static, Punctuated<IntoIter<Version, 1>>> {
    KeyValue(Cow::Borrowed(VERSION_KEY), punctuated([version]))
}

pub const fn max_frame_size(sz: u32) -> KeyValue<'static, u32> {
    KeyValue(Cow::Borrowed(MAX_FRAME_SIZE_KEY), sz)
}

pub fn capabilities(caps: &[Capability]) -> KeyValue<Punctuated<Iter<Capability>>> {
    KeyValue(Cow::Borrowed(CAPABILITIES_KEY), punctuated(caps))
}

pub const fn healthcheck(enable: bool) -> KeyValue<'static, bool> {
    KeyValue(Cow::Borrowed(HEALTHCHECK_KEY), enable)
}

pub const fn engine_id(id: &str) -> KeyValue<&str> {
    KeyValue(Cow::Borrowed(ENGINE_ID_KEY), id)
}

pub const fn status_code(code: u32) -> KeyValue<'static, u32> {
    KeyValue(Cow::Borrowed(STATUS_CODE_KEY), code)
}

pub const fn message(msg: &str) -> KeyValue<&str> {
    KeyValue(Cow::Borrowed(MSG_KEY), msg)
}
