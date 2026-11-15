//! Generational instance arena with Take-and-Restore borrowing.
//!
//! Implements the heterogeneous runtime storage specified in **RFC-0001 §6.2**.

pub mod error;
pub mod execution;
pub mod instance;
pub mod slot;
pub mod storage;

pub use super::erased::{ComponentInstance, ErasedComponent};
pub use super::flags::NodeFlags;
pub use error::ArenaError;
pub use instance::InstanceNode;
pub use storage::Arena;
