//! # Nyrui Core
//!
//! Core runtime primitives, generational arena storage, reconciliation algorithms,
//! reactivity, and layout contracts for the Nyrui declarative TUI framework.

pub mod id;
pub mod node;
pub mod props;

pub use id::{Key, NodeId, NodeKey};
pub use node::{Children, ComponentDescriptor, SlotFn, TextNode, UiNode};
pub use props::{Props, SharedStr};
