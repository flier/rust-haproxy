use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Data types
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
pub enum Type {
    /// Null type.
    Null,
    /// Boolean type.
    Boolean,
    /// 32bits signed integer
    Int32,
    /// 32bits unsigned integer  
    Uint32,
    /// 64bits signed integer
    Int64,
    /// 64bits unsigned integer
    Uint64,
    /// IPv4 address
    Ipv4,
    /// IPv6 address
    Ipv6,
    /// String type.
    String,
    /// Binary type.
    Binary,
}

impl Type {
    pub(crate) const MASK: u8 = 0x0F;
}

bitflags::bitflags! {
    /// Flags set on the SPOE frame
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Flags: u8 {
        const FALSE = 0x00;
        const TRUE = 0x10;
    }
}

impl Flags {
    pub(crate) const MASK: u8 = 0xF0;
}
