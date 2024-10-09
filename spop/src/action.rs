use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::Typed;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
pub enum Type {
    /// Set the value for an existing variable.
    SetVar = 1,
    /// Unset the value for an existing variable.
    UnsetVar,
}

/// The variable scope
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
pub enum Scope {
    Process,
    Session,
    Transaction,
    Request,
    Response,
}

/// The dynamically action on the processing of a stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Set the value for an existing variable.
    SetVar {
        /// the variable scope
        scope: Scope,
        /// the variable name
        name: String,
        /// the variable value
        value: Typed,
    },
    /// Unset the value for an existing variable.
    UnsetVar {
        /// the variable scope
        scope: Scope,
        /// the variable name
        name: String,
    },
}

impl Action {
    /// Set the value for an existing variable.
    pub fn set_var<N, V>(scope: Scope, name: N, value: V) -> Self
    where
        N: Into<String>,
        V: Into<Typed>,
    {
        Self::SetVar {
            scope,
            name: name.into(),
            value: value.into(),
        }
    }

    /// Unset the value for an existing variable.
    pub fn unset_var<N>(scope: Scope, name: N) -> Self
    where
        N: Into<String>,
    {
        Self::UnsetVar {
            scope,
            name: name.into(),
        }
    }
}
