use crate::Data;

pub const SPOE_ACT_T_SET_VAR: u8 = 1;
pub const SPOE_ACT_T_UNSET_VAR: u8 = 2;

pub const SPOE_SCOPE_PROC: u8 = 0;
pub const SPOE_SCOPE_SESS: u8 = 1;
pub const SPOE_SCOPE_TXN: u8 = 2;
pub const SPOE_SCOPE_REQ: u8 = 3;
pub const SPOE_SCOPE_RES: u8 = 4;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scope {
    Process = SPOE_SCOPE_PROC,
    Session = SPOE_SCOPE_SESS,
    Transaction = SPOE_SCOPE_TXN,
    Request = SPOE_SCOPE_REQ,
    Response = SPOE_SCOPE_RES,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    SetVar {
        scope: Scope,
        name: String,
        value: Data,
    },
    UnsetVar {
        scope: Scope,
        name: String,
    },
}
