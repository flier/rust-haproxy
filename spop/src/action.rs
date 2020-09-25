use std::mem;

use crate::{data::BufMutExt as _, varint, Data};

pub const SPOE_ACT_T_SET_VAR: u8 = 1;
pub const SPOE_ACT_T_UNSET_VAR: u8 = 2;

pub const SPOE_SCOPE_PROC: u8 = 0;
pub const SPOE_SCOPE_SESS: u8 = 1;
pub const SPOE_SCOPE_TXN: u8 = 2;
pub const SPOE_SCOPE_REQ: u8 = 3;
pub const SPOE_SCOPE_RES: u8 = 4;

/// the variable scope
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scope {
    Process = SPOE_SCOPE_PROC,
    Session = SPOE_SCOPE_SESS,
    Transaction = SPOE_SCOPE_TXN,
    Request = SPOE_SCOPE_REQ,
    Response = SPOE_SCOPE_RES,
}

/// dynamically action on the processing of a stream.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// set the value for an existing variable.
    SetVar {
        /// the variable scope
        scope: Scope,
        /// the variable name
        name: String,
        /// the variable value
        value: Data,
    },
    /// unset the value for an existing variable.
    UnsetVar {
        /// the variable scope
        scope: Scope,
        /// the variable name
        name: String,
    },
}

impl Action {
    const TYPE_SIZE: usize = mem::size_of::<u8>();
    const NB_ARGS_SIZE: usize = mem::size_of::<u8>();
    const SCOPE_SIZE: usize = mem::size_of::<Scope>();

    pub fn size(&self) -> usize {
        Self::TYPE_SIZE
            + Self::NB_ARGS_SIZE
            + Self::SCOPE_SIZE
            + match self {
                Action::SetVar { name, value, .. } => {
                    varint::size_of(name.len() as u64) + name.len() + value.size()
                }
                Action::UnsetVar { name, .. } => varint::size_of(name.len() as u64) + name.len(),
            }
    }
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
