//! Errors returned by arena operations.

use crate::id::NodeId;
use std::fmt;

/// Errors returned by arena operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaError {
    /// The specified node was not found or the generation is stale.
    NodeNotFound(NodeId),
    /// The component instance is currently taken out by another active invocation.
    ComponentAlreadyBorrowed(NodeId),
    /// Properties or message downcasting failed due to a type mismatch.
    DowncastFailed,
}

impl fmt::Display for ArenaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeNotFound(id) => {
                write!(f, "Node {id:?} not found in arena (stale or removed)")
            }
            Self::ComponentAlreadyBorrowed(id) => {
                write!(f, "Component {id:?} is already currently borrowed")
            }
            Self::DowncastFailed => write!(f, "Type mismatch during component downcast"),
        }
    }
}

impl std::error::Error for ArenaError {}
