//! Generational reactive signal and computed storage with fine-grained dependency tracking.
//!
//! This module implements the reactive graph specified in **RFC-0001 §7.1**:
//! - [`Signal`]: A lightweight, `Copy` handle to a generational signal in [`ReactiveGraph`].
//! - [`SignalId`]: Unique generational identifier referencing a signal slot in [`ReactiveGraph`].
//! - [`Computed`]: A lightweight, `Copy` handle to a derived computation in [`ReactiveGraph`].
//! - [`ComputedId`]: Unique generational identifier referencing a computed slot in [`ReactiveGraph`].
//! - [`ReactiveGraph`]: Generational slot arena maintaining reactive state, subscriber sets, and tracking stacks.
//! - [`ReactiveContext`]: Trait providing uniform access to the active [`ReactiveGraph`].
//! - [`ObserverId`]: Entity observing signal reads for fine-grained dependency tracking.
//! - [`SignalError`]: Error conditions encountered during reactive access and lifecycle dispatch.

pub mod computed;
pub mod context;
pub mod error;
pub mod graph;
pub mod signal;

pub use computed::{Computed, ComputedId};
pub use context::ReactiveContext;
pub use error::SignalError;
pub use graph::{ComputedState, ObserverId, ReactiveGraph};
pub use signal::{Signal, SignalId};
