//! Scoped slot closures for dynamic template rendering.

use crate::node::ui_node::UiNode;

use std::fmt;
use std::rc::Rc;

/// A single-threaded scoped slot closure for dynamic list or table rendering (RFC-0001 §5.3).
///
/// Because Nyrui's runtime arena is single-threaded (`!Send`), `SlotFn` wraps an [`Rc`]
/// closure rather than requiring `Arc<dyn Fn + Send + Sync>`.
pub struct SlotFn<T: ?Sized>(Rc<dyn Fn(&T) -> UiNode>);

impl<T: ?Sized> Clone for SlotFn<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: ?Sized> SlotFn<T> {
    /// Creates a new scoped slot closure.
    #[inline]
    #[must_use]
    pub fn new(f: impl Fn(&T) -> UiNode + 'static) -> Self {
        Self(Rc::new(f))
    }

    /// Invokes the slot closure with the provided item reference, generating a [`UiNode`].
    #[inline]
    #[must_use]
    pub fn call(&self, item: &T) -> UiNode {
        (self.0)(item)
    }

    /// Alias for [`SlotFn::call`] for template rendering ergonomics.
    #[inline]
    #[must_use]
    pub fn render(&self, item: &T) -> UiNode {
        self.call(item)
    }
}

impl<T: ?Sized> fmt::Debug for SlotFn<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlotFn").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_fn_evaluation_and_cloning() {
        struct User {
            pub id: u64,
            pub name: &'static str,
        }

        let slot = SlotFn::new(|user: &User| UiNode::text(format!("{}: {}", user.id, user.name)));
        let slot_clone = slot.clone();

        let user = User {
            id: 101,
            name: "Alice",
        };
        let rendered = slot_clone.render(&user);

        if let UiNode::Text(text) = rendered {
            assert_eq!(text.content, "101: Alice");
        } else {
            panic!("expected Text node");
        }
    }
}
