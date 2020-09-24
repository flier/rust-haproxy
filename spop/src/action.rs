use crate::{data::BufMutExt as _, Data};

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

pub trait BufMutExt {
    fn put_action(&mut self, action: Action);
}

impl<T> BufMutExt for T
where
    T: bytes::BufMut,
{
    fn put_action(&mut self, action: Action) {
        match action {
            Action::SetVar { scope, name, value } => {
                self.put_slice(&[SPOE_ACT_T_SET_VAR, 3, scope as u8]);
                self.put_str(name);
                self.put_data(value);
            }
            Action::UnsetVar { scope, name } => {
                self.put_slice(&[SPOE_ACT_T_UNSET_VAR, 2, scope as u8]);
                self.put_str(name);
            }
        }
    }
}
