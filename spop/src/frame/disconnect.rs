use crate::Error;

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
    pub fn new<S: Into<String>>(status: Error, reason: S) -> Self {
        Self {
            status_code: status as u32,
            message: reason.into(),
        }
    }
}
