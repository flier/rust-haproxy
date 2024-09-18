use crate::{data::Value, frame::kv};

/// If an error occurs, at anytime, from the HAProxy/agent side,
/// a HAPROXY-DISCONNECT/AGENT-DISCONNECT frame is sent with information describing the error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disconnect {
    /// This is the code corresponding to the error.
    pub status_code: u32,
    /// This is the code corresponding to the error.
    pub message: String,
}

impl Disconnect {
    pub(crate) fn size(&self) -> usize {
        kv::status_code(self.status_code).size() + kv::message(&self.message).size()
    }
}
