//! Single-threaded event callbacks for upward child-to-parent communication.
//!
//! Provides the [`Callback`] primitive specified in **RFC-0001 §8.2**.

use std::fmt;
use std::rc::Rc;

/// A lightweight, cheaply clonable upward callback wrapper.
///
/// Implements child-to-parent message communication without lifetime contagion.
/// Props containing a [`Callback`] satisfy `'static` so they can be retained
/// inside component arena slots across frame renders.
///
/// # Equality
///
/// [`Callback`] implements [`PartialEq`] using pointer equality ([`Rc::ptr_eq`]).
/// When child components compare their properties during reconciliation via
/// [`crate::props::Props::props_eq`], unchanged callbacks evaluate
/// as equal in $O(1)$ time, skipping unnecessary subtree re-renders.
pub struct Callback<In: 'static = ()>(Rc<dyn Fn(In) + 'static>);

impl<In: 'static> Callback<In> {
    /// Creates a new [`Callback`] wrapping the provided closure.
    #[inline]
    #[must_use]
    pub fn new(f: impl Fn(In) + 'static) -> Self {
        Self(Rc::new(f))
    }

    /// Creates a no-op [`Callback`] that silently discards all inputs.
    #[inline]
    #[must_use]
    pub fn noop() -> Self {
        Self(Rc::new(|_| ()))
    }

    /// Emits an input value to the callback closure.
    #[inline]
    pub fn emit(&self, input: In) {
        (self.0)(input);
    }

    /// Transforms input of type `NewIn` into `In` before passing it to this callback.
    ///
    /// This combinator allows child components to adapt their local event data
    /// into the parent's expected message schema without modifying parent handlers.
    #[inline]
    #[must_use]
    pub fn reform<NewIn: 'static>(&self, f: impl Fn(NewIn) -> In + 'static) -> Callback<NewIn> {
        let parent = Rc::clone(&self.0);
        Callback::new(move |new_in| {
            parent(f(new_in));
        })
    }
}

impl<In: 'static> Clone for Callback<In> {
    #[inline]
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<In: 'static> Default for Callback<In> {
    #[inline]
    fn default() -> Self {
        Self::noop()
    }
}

impl<In: 'static> fmt::Debug for Callback<In> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Callback").finish()
    }
}

impl<In: 'static> PartialEq for Callback<In> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<In: 'static> Eq for Callback<In> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn callback_emission() {
        let cell = Rc::new(Cell::new(0));
        let c = cell.clone();
        let cb = Callback::new(move |val: i32| {
            c.set(c.get() + val);
        });

        cb.emit(10);
        cb.emit(5);
        assert_eq!(cell.get(), 15);
    }

    #[test]
    fn callback_reform() {
        let cell = Rc::new(Cell::new(0));
        let c = cell.clone();
        let parent_cb = Callback::new(move |val: i32| {
            c.set(val);
        });

        let reformed: Callback<String> = parent_cb.reform(|s: String| s.len() as i32);
        reformed.emit("hello".to_string());
        assert_eq!(cell.get(), 5);
    }

    #[test]
    fn callback_pointer_equality() {
        let cb1 = Callback::<()>::noop();
        let cb2 = cb1.clone();
        let cb3 = Callback::<()>::noop();

        assert_eq!(cb1, cb2);
        assert_ne!(cb1, cb3);
    }
}
