//! Generational reactive signal storage and fine-grained dependency tracking.
//!
//! This module implements the reactive graph specified in **RFC-0001 §7.1**:
//! - [`Signal`]: A lightweight, `Copy` handle to a generational signal in [`ReactiveGraph`].
//! - [`SignalId`]: Unique generational identifier referencing a signal slot in [`ReactiveGraph`].
//! - [`ReactiveGraph`]: Generational slot arena maintaining signal values, subscriber sets, and tracking stacks.
//! - [`ReactiveContext`]: Trait providing uniform access to the active [`ReactiveGraph`].
//! - [`ObserverId`]: Entity observing signal reads for fine-grained dependency tracking.
//! - [`SignalError`]: Error conditions encountered during signal access and lifecycle dispatch.

pub mod context;
pub mod error;
pub mod graph;
pub mod signal;

pub use context::ReactiveContext;
pub use error::SignalError;
pub use graph::{ObserverId, ReactiveGraph};
pub use signal::{Signal, SignalId};
