//! Generational signal handle and typed access primitives.

use super::context::ReactiveContext;
use super::error::SignalError;

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::num::NonZeroU32;

/// A generational identifier uniquely referencing a signal slot in a [`super::ReactiveGraph`].
///
/// Similar to [`crate::id::NodeId`], `SignalId` pairs a slot `index` with an ABA-preventing
/// `generation` counter, ensuring stale handles safely fail lookup rather than aliasing recycled slots.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct SignalId {
    /// Slot index of the signal within the runtime reactive graph.
    pub index: u32,
    /// Generational counter preventing stale references upon slot reuse.
    pub generation: NonZeroU32,
}

impl SignalId {
    /// Creates a new [`SignalId`] with the given slot index and generation.
    #[inline]
    #[must_use]
    pub const fn new(index: u32, generation: NonZeroU32) -> Self {
        Self { index, generation }
    }
}

impl fmt::Display for SignalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SignalId({}:{})", self.index, self.generation)
    }
}

/// A lightweight, cheaply `Copy` handle to a generational reactive signal.
///
/// Implements the signal handle specification in **RFC-0001 §7.1**.
///
/// `Signal<T>` contains only its 8-byte [`SignalId`] and phantom data.
/// It implements [`Copy`], [`Clone`], [`Eq`], and [`PartialEq`] unconditionally,
/// regardless of whether the contained type `T` implements them.
pub struct Signal<T: 'static> {
    pub(crate) id: SignalId,
    _marker: PhantomData<fn() -> T>,
}

impl<T: 'static> Signal<T> {
    /// Creates a new [`Signal`] holding `initial_value` in the provided [`ReactiveContext`].
    #[inline]
    #[must_use]
    pub fn new(cx: &impl ReactiveContext, initial_value: T) -> Self {
        cx.reactive_graph().create_signal(initial_value)
    }

    /// Creates a [`Signal`] handle from an existing [`SignalId`].
    #[inline]
    #[must_use]
    pub const fn from_id(id: SignalId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    /// Returns the underlying generational [`SignalId`].
    #[inline]
    #[must_use]
    pub const fn id(&self) -> SignalId {
        self.id
    }

    /// Reads and clones the current value of the signal.
    ///
    /// If invoked within an active observer scope (e.g. during component rendering
    /// or inside a computed expression), this read registers a dependency on this signal.
    ///
    /// # Panics
    ///
    /// Panics if the signal is stale (unmounted/recycled) or if the value is already borrowed mutably.
    #[inline]
    #[must_use]
    pub fn get<Cx: ReactiveContext + ?Sized>(&self, cx: &Cx) -> T
    where
        T: Clone,
    {
        cx.reactive_graph().get_signal(*self)
    }

    /// Attempts to read and clone the current value of the signal without panicking.
    #[inline]
    pub fn try_get<Cx: ReactiveContext + ?Sized>(&self, cx: &Cx) -> Result<T, SignalError>
    where
        T: Clone,
    {
        cx.reactive_graph().try_get_signal(*self)
    }

    /// Applies a closure to an immutable borrowed reference of the signal's value.
    ///
    /// If invoked within an active observer scope, records a reactive dependency on this signal.
    ///
    /// # Panics
    ///
    /// Panics if the signal is stale or if the value is already borrowed mutably.
    #[inline]
    pub fn with<R, Cx: ReactiveContext + ?Sized>(&self, cx: &Cx, f: impl FnOnce(&T) -> R) -> R {
        cx.reactive_graph().with_signal(*self, f)
    }

    /// Attempts to borrow the signal's value immutably without panicking.
    #[inline]
    pub fn try_with<R, Cx: ReactiveContext + ?Sized>(
        &self,
        cx: &Cx,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, SignalError> {
        cx.reactive_graph().try_with_signal(*self, f)
    }

    /// Sets a new value for the signal, unconditionally marking all dependent subscribers dirty.
    ///
    /// To skip invalidating subscribers when the new value is equal to the current value,
    /// use [`Signal::set_if_changed`] when `T: PartialEq`.
    ///
    /// # Panics
    ///
    /// Panics if the signal is stale or if the value is currently borrowed.
    #[inline]
    pub fn set<Cx: ReactiveContext + ?Sized>(&self, cx: &Cx, new_val: T) {
        cx.reactive_graph().set_signal(*self, new_val);
    }

    /// Attempts to set a new value for the signal without panicking.
    #[inline]
    pub fn try_set<Cx: ReactiveContext + ?Sized>(
        &self,
        cx: &Cx,
        new_val: T,
    ) -> Result<(), SignalError> {
        cx.reactive_graph().try_set_signal(*self, new_val)
    }

    /// Mutates the signal's value in place via a closure, unconditionally marking subscribers dirty.
    ///
    /// # Panics
    ///
    /// Panics if the signal is stale or if the value is currently borrowed.
    #[inline]
    pub fn update<R, Cx: ReactiveContext + ?Sized>(
        &self,
        cx: &Cx,
        f: impl FnOnce(&mut T) -> R,
    ) -> R {
        cx.reactive_graph().update_signal(*self, f)
    }

    /// Attempts to mutate the signal's value in place without panicking.
    #[inline]
    pub fn try_update<R, Cx: ReactiveContext + ?Sized>(
        &self,
        cx: &Cx,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, SignalError> {
        cx.reactive_graph().try_update_signal(*self, f)
    }

    /// Replaces the signal's value with `new_val`, returning the previous value,
    /// and marks all dependent subscribers dirty.
    ///
    /// # Panics
    ///
    /// Panics if the signal is stale or if the value is currently borrowed.
    #[inline]
    pub fn replace<Cx: ReactiveContext + ?Sized>(&self, cx: &Cx, new_val: T) -> T {
        cx.reactive_graph().replace_signal(*self, new_val)
    }

    /// Attempts to replace the signal's value without panicking.
    #[inline]
    pub fn try_replace<Cx: ReactiveContext + ?Sized>(
        &self,
        cx: &Cx,
        new_val: T,
    ) -> Result<T, SignalError> {
        cx.reactive_graph().try_replace_signal(*self, new_val)
    }

    /// Disposes of the signal, freeing its slot in the reactive graph and incrementing its generation.
    ///
    /// Subsequent operations on this signal handle will fail with [`SignalError::SignalNotFound`].
    #[inline]
    pub fn dispose<Cx: ReactiveContext + ?Sized>(&self, cx: &Cx) -> bool {
        cx.reactive_graph().dispose_signal(self.id)
    }
}

impl<T: PartialEq + 'static> Signal<T> {
    /// Compares `new_val` with the current value using [`PartialEq`].
    ///
    /// If `current == new_val`, the write is skipped, dependent subscribers are **not** marked dirty,
    /// and `false` is returned.
    ///
    /// If `current != new_val`, the value is updated, dependent subscribers are marked dirty,
    /// and `true` is returned.
    ///
    /// # Panics
    ///
    /// Panics if the signal is stale or if the value is currently borrowed.
    #[inline]
    pub fn set_if_changed<Cx: ReactiveContext + ?Sized>(&self, cx: &Cx, new_val: T) -> bool {
        cx.reactive_graph().set_signal_if_changed(*self, new_val)
    }

    /// Attempts to compare and set `new_val` without panicking.
    #[inline]
    pub fn try_set_if_changed<Cx: ReactiveContext + ?Sized>(
        &self,
        cx: &Cx,
        new_val: T,
    ) -> Result<bool, SignalError> {
        cx.reactive_graph()
            .try_set_signal_if_changed(*self, new_val)
    }
}

impl<T: 'static> Clone for Signal<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for Signal<T> {}

impl<T: 'static> PartialEq for Signal<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: 'static> Eq for Signal<T> {}

impl<T: 'static> Hash for Signal<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T: 'static> PartialOrd for Signal<T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: 'static> Ord for Signal<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl<T: 'static> fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signal")
            .field("id", &self.id)
            .field("type", &std::any::type_name::<T>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[expect(dead_code)]
    struct NonCloneType(pub u32);

    #[test]
    fn signal_is_copy_and_clone_for_non_clone_type() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<Signal<NonCloneType>>();

        let id = SignalId::new(1, NonZeroU32::new(1).expect("non-zero"));
        let sig: Signal<NonCloneType> = Signal::from_id(id);
        let sig_copy = sig;
        assert_eq!(sig, sig_copy);
    }

    #[test]
    fn signal_equality_and_hashing() {
        use std::collections::HashSet;

        let id1 = SignalId::new(1, NonZeroU32::new(1).expect("non-zero"));
        let id2 = SignalId::new(1, NonZeroU32::new(2).expect("non-zero"));
        let id3 = SignalId::new(2, NonZeroU32::new(1).expect("non-zero"));

        let sig1: Signal<i32> = Signal::from_id(id1);
        let sig1_dup: Signal<i32> = Signal::from_id(id1);
        let sig2: Signal<i32> = Signal::from_id(id2);
        let sig3: Signal<i32> = Signal::from_id(id3);

        assert_eq!(sig1, sig1_dup);
        assert_ne!(sig1, sig2);
        assert_ne!(sig1, sig3);

        let mut set = HashSet::new();
        set.insert(sig1);
        set.insert(sig1_dup);
        set.insert(sig2);
        set.insert(sig3);
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn signal_id_niche_optimization() {
        use std::mem::size_of;
        assert_eq!(size_of::<SignalId>(), 8);
        assert_eq!(size_of::<Option<SignalId>>(), 8);
        assert_eq!(size_of::<Signal<i32>>(), 8);
        assert_eq!(size_of::<Option<Signal<i32>>>(), 8);
    }
}
