//! Errors returned by reactive graph and signal operations.

use crate::reactivity::SignalId;
use std::fmt;

/// Errors returned by [`super::ReactiveGraph`] and [`super::Signal`] operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignalError {
    /// The specified signal was not found or its generational counter is stale.
    SignalNotFound(SignalId),
    /// Type mismatch when downcasting the signal value.
    TypeMismatch {
        /// The expected type name registered in the signal storage.
        expected: &'static str,
        /// The type name requested by the caller.
        found: &'static str,
    },
    /// The signal value is currently borrowed and cannot be borrowed again with the requested exclusivity.
    AlreadyBorrowed(SignalId),
}

impl fmt::Display for SignalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SignalNotFound(id) => {
                write!(
                    f,
                    "Signal {id:?} not found in reactive graph (stale or removed)"
                )
            }
            Self::TypeMismatch { expected, found } => {
                write!(
                    f,
                    "Type mismatch for signal: expected `{expected}`, found `{found}`"
                )
            }
            Self::AlreadyBorrowed(id) => {
                write!(f, "Signal {id:?} is already borrowed")
            }
        }
    }
}

impl std::error::Error for SignalError {}
