use parse_display::{Display, FromStr};

/// The capabilities supported by HAProxy
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Display, FromStr)]
#[display(style = "snake_case")]
pub enum Capability {
    /// This is the ability for a peer to support fragmented payload in received frames.
    Fragmentation,
    ///  This is the ability for a peer to decouple NOTIFY and ACK frames.
    Pipelining,
    /// This ability is similar to the pipelining, but here any TCP connection established
    /// between HAProxy and the agent can be used to send ACK frames.
    Async,
}
