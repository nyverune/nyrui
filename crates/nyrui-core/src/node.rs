//! Declarative UI Description Layer.
//!
//! This module implements the intermediate representation tree specified in
//! **RFC-0001 §4.1 and §5.3**:
//! - [`UiNode`]: Declarative description of an unmounted visual node.
//! - [`ComponentDescriptor`]: Type-erased description of a component, its properties, and its identity key.
//! - [`TextNode`]: Lightweight text node holding a [`SharedStr`](crate::props::SharedStr).
//! - [`Children`]: Small-vector-backed container optimizing child collections.
//! - [`SlotFn`]: Single-threaded scoped slot closure generating dynamic subtrees from contextual data.

pub mod children;
pub mod component;
pub mod slot;
pub mod text;
pub mod ui_node;

pub use children::Children;
pub use component::ComponentDescriptor;
pub use slot::SlotFn;
pub use text::TextNode;
pub use ui_node::UiNode;
