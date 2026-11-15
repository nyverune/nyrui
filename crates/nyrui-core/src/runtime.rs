//! Runtime storage, generational arena, and execution primitives.
//!
//! Provides the core runtime data structures specified in **RFC-0001 §6.2**:
//! - [`Arena`]: Single-threaded generational arena mapping `NodeId -> InstanceNode`.
//! - [`InstanceNode`]: Mounted component metadata and type-erased instance container.
//! - [`NodeFlags`]: Bitflags representing dirty, focus, and mounted states.
//! - [`ErasedComponent`]: Object-safe interface for heterogeneous components.

pub mod arena;
pub mod erased;
pub mod flags;

pub use arena::{Arena, ArenaError, InstanceNode};
pub use erased::{ComponentInstance, ErasedComponent};
pub use flags::NodeFlags;
