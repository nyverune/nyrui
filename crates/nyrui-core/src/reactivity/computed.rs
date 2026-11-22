//! Generational handle and access primitives for derived reactive computations.

use super::context::ReactiveContext;
use super::error::SignalError;
use super::graph::ReactiveGraph;

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::num::NonZeroU32;

/// A generational identifier referencing a computed slot in a [`ReactiveGraph`].
///
/// Like [`crate::id::NodeId`] and [`super::SignalId`], `ComputedId` pairs a slot `index`
/// with an ABA-preventing `generation` counter.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ComputedId {
    /// Slot index within the reactive graph's computed storage.
    pub index: u32,
    /// Generational counter preventing stale references upon slot reuse.
    pub generation: NonZeroU32,
}

impl ComputedId {
    /// Creates a new [`ComputedId`] with the given slot index and generation.
    #[inline]
    #[must_use]
    pub const fn new(index: u32, generation: NonZeroU32) -> Self {
        Self { index, generation }
    }
}

impl fmt::Display for ComputedId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ComputedId({}:{})", self.index, self.generation)
    }
}

/// A lightweight, cheaply `Copy` handle to a derived reactive computation.
///
/// Implements derived computations specified in **RFC-0001 §7.1**:
/// - **Push-Pull Evaluation:** When a dependency signal updates, dependent computed nodes
///   are eagerly marked dirty and downstream components are scheduled, but the computation
///   closure is only re-evaluated lazily when read.
/// - **Diamond Dependency Resolution:** In diamond dependency graphs (`A -> B, A -> C, (B, C) -> D`),
///   `D` recomputes at most once per batch without redundant evaluation.
/// - **Cycle Detection:** Transitive dependency cycles are detected via the reactive tracking stack
///   and panic with a structured diagnostic trace.
pub struct Computed<T: 'static> {
    pub(crate) id: ComputedId,
    _marker: PhantomData<fn() -> T>,
}

impl<T: 'static> Copy for Computed<T> {}

impl<T: 'static> Clone for Computed<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> PartialEq for Computed<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: 'static> Eq for Computed<T> {}

impl<T: 'static> Hash for Computed<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T: 'static> fmt::Debug for Computed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Computed").field(&self.id).finish()
    }
}

impl<T: 'static> Computed<T> {
    /// Creates a new [`Computed`] expression in the provided [`ReactiveContext`].
    ///
    /// Evaluation is lazy: the closure is not invoked until the first read.
    #[inline]
    #[must_use]
    pub fn new<Cx: ReactiveContext + ?Sized>(cx: &Cx, compute: impl Fn() -> T + 'static) -> Self {
        cx.reactive_graph().create_computed(compute)
    }

    /// Creates a new [`Computed`] expression whose closure receives a reference to the active [`ReactiveGraph`].
    #[inline]
    #[must_use]
    pub fn with_context<Cx: ReactiveContext + ?Sized>(
        cx: &Cx,
        compute: impl Fn(&ReactiveGraph) -> T + 'static,
    ) -> Self {
        cx.reactive_graph().create_computed_with_context(compute)
    }

    /// Creates a [`Computed`] handle from an existing [`ComputedId`].
    #[inline]
    #[must_use]
    pub const fn from_id(id: ComputedId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    /// Returns the underlying generational [`ComputedId`].
    #[inline]
    #[must_use]
    pub const fn id(&self) -> ComputedId {
        self.id
    }

    /// Reads and clones the current cached value of the computation, re-evaluating if dirty.
    ///
    /// If invoked within an active observer scope, records a dependency on this computed node.
    ///
    /// # Panics
    ///
    /// Panics if a dependency cycle is detected, if the computed node is stale, or if types mismatch.
    #[inline]
    #[must_use]
    pub fn get<Cx: ReactiveContext + ?Sized>(&self, cx: &Cx) -> T
    where
        T: Clone,
    {
        cx.reactive_graph().get_computed(*self)
    }

    /// Attempts to read and clone the current value of the computation without panicking on lookup errors.
    ///
    /// # Panics
    ///
    /// Panics if a dependency cycle is detected.
    #[inline]
    pub fn try_get<Cx: ReactiveContext + ?Sized>(&self, cx: &Cx) -> Result<T, SignalError>
    where
        T: Clone,
    {
        cx.reactive_graph().try_get_computed(*self)
    }

    /// Applies a closure to an immutable borrowed reference of the computed value, re-evaluating if dirty.
    ///
    /// If invoked within an active observer scope, records a dependency on this computed node.
    ///
    /// # Panics
    ///
    /// Panics if a dependency cycle is detected, if the computed node is stale, or if types mismatch.
    #[inline]
    pub fn with<R, Cx: ReactiveContext + ?Sized>(&self, cx: &Cx, f: impl FnOnce(&T) -> R) -> R {
        cx.reactive_graph().with_computed(*self, f)
    }

    /// Attempts to borrow the computed value immutably without panicking on lookup errors.
    ///
    /// # Panics
    ///
    /// Panics if a dependency cycle is detected.
    #[inline]
    pub fn try_with<R, Cx: ReactiveContext + ?Sized>(
        &self,
        cx: &Cx,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, SignalError> {
        cx.reactive_graph().try_with_computed(*self, f)
    }

    /// Reads and clones the current computed value without registering an active dependency.
    #[inline]
    #[must_use]
    pub fn get_untracked<Cx: ReactiveContext + ?Sized>(&self, cx: &Cx) -> T
    where
        T: Clone,
    {
        cx.reactive_graph().get_computed_untracked(*self)
    }

    /// Applies a closure to an immutable borrowed reference without registering an active dependency.
    #[inline]
    pub fn with_untracked<R, Cx: ReactiveContext + ?Sized>(
        &self,
        cx: &Cx,
        f: impl FnOnce(&T) -> R,
    ) -> R {
        cx.reactive_graph().with_computed_untracked(*self, f)
    }
}
