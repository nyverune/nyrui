//! Execution and render contexts exposed to components during their lifecycle.

use crate::id::NodeId;

use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;

/// Mutable execution context passed to [`Component`](super::Component) lifecycle and update methods.
///
/// Provides access to the component's unique arena [`NodeId`] and enables manual
/// invalidation to schedule re-renders.
pub struct ComponentContext<'a, C: ?Sized> {
    node_id: NodeId,
    invalidated: Option<&'a Cell<bool>>,
    _marker: PhantomData<fn() -> C>,
}

impl<'a, C: ?Sized> ComponentContext<'a, C> {
    /// Creates a new [`ComponentContext`] bound to a runtime node and an invalidation cell.
    #[inline]
    #[must_use]
    pub fn new(node_id: NodeId, invalidated: &'a Cell<bool>) -> Self {
        Self {
            node_id,
            invalidated: Some(invalidated),
            _marker: PhantomData,
        }
    }

    /// Creates a standalone [`ComponentContext`] for testing without an external runtime.
    #[inline]
    #[must_use]
    pub fn for_node(node_id: NodeId) -> ComponentContext<'static, C> {
        ComponentContext {
            node_id,
            invalidated: None,
            _marker: PhantomData,
        }
    }

    /// Returns the unique [`NodeId`] of the component instance in the arena.
    #[inline]
    #[must_use]
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Marks this component instance as dirty, scheduling it for re-rendering in the next frame.
    #[inline]
    pub fn invalidate(&mut self) {
        if let Some(cell) = self.invalidated {
            cell.set(true);
        }
    }

    /// Returns whether this context has been marked dirty for re-rendering.
    #[inline]
    #[must_use]
    pub fn is_invalidated(&self) -> bool {
        self.invalidated.is_some_and(|c| c.get())
    }

    /// Re-binds this execution context to another component type without changing the underlying state.
    #[inline]
    #[must_use]
    pub fn rebind<NewC: ?Sized>(&mut self) -> ComponentContext<'_, NewC> {
        ComponentContext {
            node_id: self.node_id,
            invalidated: self.invalidated,
            _marker: PhantomData,
        }
    }
}

impl<C: ?Sized> fmt::Debug for ComponentContext<'_, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentContext")
            .field("node_id", &self.node_id)
            .field("is_invalidated", &self.is_invalidated())
            .finish()
    }
}

/// Read-only rendering context passed to [`Component::render`](super::Component::render).
///
/// Exposes the component's [`NodeId`] and allows the component or its children
/// to schedule re-renders if necessary during layout or render evaluation.
pub struct RenderContext<'a, C: ?Sized> {
    node_id: NodeId,
    invalidated: Option<&'a Cell<bool>>,
    _marker: PhantomData<fn() -> C>,
}

impl<'a, C: ?Sized> RenderContext<'a, C> {
    /// Creates a new [`RenderContext`] bound to a runtime node and an invalidation cell.
    #[inline]
    #[must_use]
    pub fn new(node_id: NodeId, invalidated: &'a Cell<bool>) -> Self {
        Self {
            node_id,
            invalidated: Some(invalidated),
            _marker: PhantomData,
        }
    }

    /// Creates a standalone [`RenderContext`] for testing without an external runtime.
    #[inline]
    #[must_use]
    pub fn for_node(node_id: NodeId) -> RenderContext<'static, C> {
        RenderContext {
            node_id,
            invalidated: None,
            _marker: PhantomData,
        }
    }

    /// Returns the unique [`NodeId`] of the component being rendered.
    #[inline]
    #[must_use]
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Marks this component instance as dirty, scheduling it for re-rendering in the next frame.
    #[inline]
    pub fn invalidate(&self) {
        if let Some(cell) = self.invalidated {
            cell.set(true);
        }
    }

    /// Returns whether this context has been marked dirty for re-rendering.
    #[inline]
    #[must_use]
    pub fn is_invalidated(&self) -> bool {
        self.invalidated.is_some_and(|c| c.get())
    }

    /// Re-binds this render context to another component type without changing the underlying state.
    #[inline]
    #[must_use]
    pub fn rebind<NewC: ?Sized>(&self) -> RenderContext<'_, NewC> {
        RenderContext {
            node_id: self.node_id,
            invalidated: self.invalidated,
            _marker: PhantomData,
        }
    }
}

impl<C: ?Sized> fmt::Debug for RenderContext<'_, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderContext")
            .field("node_id", &self.node_id)
            .field("is_invalidated", &self.is_invalidated())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    struct Dummy;

    fn test_node_id() -> NodeId {
        NodeId::new(42, NonZeroU32::new(1).expect("non-zero"))
    }

    #[test]
    fn component_context_invalidation() {
        let flag = Cell::new(false);
        let id = test_node_id();
        let mut ctx = ComponentContext::<Dummy>::new(id, &flag);

        assert_eq!(ctx.node_id(), id);
        assert!(!ctx.is_invalidated());

        ctx.invalidate();
        assert!(ctx.is_invalidated());
        assert!(flag.get());
    }

    #[test]
    fn render_context_invalidation() {
        let flag = Cell::new(false);
        let id = test_node_id();
        let ctx = RenderContext::<Dummy>::new(id, &flag);

        assert_eq!(ctx.node_id(), id);
        assert!(!ctx.is_invalidated());

        ctx.invalidate();
        assert!(ctx.is_invalidated());
        assert!(flag.get());
    }

    #[test]
    fn standalone_context_without_cell() {
        let id = test_node_id();
        let mut comp_ctx = ComponentContext::<Dummy>::for_node(id);
        assert_eq!(comp_ctx.node_id(), id);
        assert!(!comp_ctx.is_invalidated());
        comp_ctx.invalidate();
        assert!(!comp_ctx.is_invalidated());

        let render_ctx = RenderContext::<Dummy>::for_node(id);
        assert_eq!(render_ctx.node_id(), id);
        assert!(!render_ctx.is_invalidated());
        render_ctx.invalidate();
        assert!(!render_ctx.is_invalidated());
    }

    #[test]
    fn context_rebind_preserves_identity_and_invalidation() {
        struct OtherComponent;

        let flag = Cell::new(false);
        let id = test_node_id();

        // 1. ComponentContext rebind
        let mut comp_ctx = ComponentContext::<Dummy>::new(id, &flag);
        assert_eq!(comp_ctx.node_id(), id);
        {
            let mut rebound = comp_ctx.rebind::<OtherComponent>();
            assert_eq!(rebound.node_id(), id);
            assert!(!rebound.is_invalidated());
            rebound.invalidate();
            assert!(rebound.is_invalidated());
        }
        assert!(comp_ctx.is_invalidated());
        assert!(flag.get());

        // 2. RenderContext rebind
        let render_flag = Cell::new(false);
        let render_ctx = RenderContext::<Dummy>::new(id, &render_flag);
        assert_eq!(render_ctx.node_id(), id);
        {
            let rebound_render = render_ctx.rebind::<OtherComponent>();
            assert_eq!(rebound_render.node_id(), id);
            assert!(!rebound_render.is_invalidated());
            rebound_render.invalidate();
            assert!(rebound_render.is_invalidated());
        }
        assert!(render_ctx.is_invalidated());
        assert!(render_flag.get());
    }
}
