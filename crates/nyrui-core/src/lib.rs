//! # Nyrui Core
//!
//! Core runtime primitives, generational arena storage, reconciliation algorithms,
//! reactivity, and layout contracts for the Nyrui declarative TUI framework.

pub mod id;
pub mod props;

pub use id::{Key, NodeId, NodeKey};
pub use props::{Props, SharedStr};
