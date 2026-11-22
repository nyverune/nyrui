//! Errors returned by reactive graph and signal operations.

use crate::reactivity::{ComputedId, SignalId};
use std::fmt;

/// Errors returned by [`super::ReactiveGraph`], [`super::Signal`], and [`super::Computed`] operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignalError {
    /// The specified signal was not found or its generational counter is stale.
    SignalNotFound(SignalId),
    /// Type mismatch when downcasting the signal or computed value.
    TypeMismatch {
        /// The expected type name registered in the signal storage.
        expected: &'static str,
        /// The type name requested by the caller.
        found: &'static str,
    },
    /// The signal value is currently borrowed and cannot be borrowed again with the requested exclusivity.
    AlreadyBorrowed(SignalId),
    /// The specified computed computation was not found or its generational counter is stale.
    ComputedNotFound(ComputedId),
    /// The computed computation is currently borrowed during evaluation.
    ComputedAlreadyBorrowed(ComputedId),
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
                    "Type mismatch for reactive value: expected `{expected}`, found `{found}`"
                )
            }
            Self::AlreadyBorrowed(id) => {
                write!(f, "Signal {id:?} is already borrowed")
            }
            Self::ComputedNotFound(id) => {
                write!(
                    f,
                    "Computed {id:?} not found in reactive graph (stale or removed)"
                )
            }
            Self::ComputedAlreadyBorrowed(id) => {
                write!(f, "Computed {id:?} is already borrowed")
            }
        }
    }
}

impl std::error::Error for SignalError {}
