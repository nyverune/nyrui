//! # Nyrui Core
//!
//! Core runtime primitives, generational arena storage, reconciliation algorithms,
//! reactivity, and layout contracts for the Nyrui declarative TUI framework.

pub mod callback;
pub mod component;
pub mod id;
pub mod node;
pub mod props;
pub mod reactivity;
pub mod runtime;

pub use callback::Callback;
pub use component::{Component, ComponentContext, RenderContext, ShouldRender};
pub use id::{Key, NodeId, NodeKey};
pub use node::{Children, ComponentDescriptor, SlotFn, TextNode, UiNode};
pub use props::{Props, SharedStr};
pub use reactivity::{
    Computed, ComputedId, ComputedState, ObserverId, ReactiveContext, ReactiveGraph, Signal,
    SignalError, SignalId,
};
pub use runtime::{
    Arena, ArenaError, InstanceNode, MessageQueue, NodeFlags, QueuedMessage, Runtime,
    RuntimeBatchContext, ScheduledNode, Scheduler, TaskTracker, UiMessage, UiReceiver, UiSendError,
    UiSender,
};
