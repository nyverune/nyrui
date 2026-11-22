//! Execution and render contexts exposed to components during their lifecycle.

use super::Component;
use crate::callback::Callback;
use crate::id::NodeId;
use crate::reactivity::{ReactiveContext, ReactiveGraph};
use crate::runtime::arena::Arena;
use crate::runtime::channel::UiSender;
use crate::runtime::message::MessageQueue;
use crate::runtime::task::TaskTracker;

use std::any::TypeId;
use std::cell::Cell;
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;

/// Mutable execution context passed to [`super::Component`] lifecycle and update methods.
///
/// Provides access to the component's unique arena [`NodeId`], attached scoped contexts,
/// and enables manual invalidation to schedule re-renders.
pub struct ComponentContext<'a, C: ?Sized> {
    node_id: NodeId,
    invalidated: Option<&'a Cell<bool>>,
    message_queue: Option<MessageQueue>,
    arena: Option<&'a Arena>,
    reactive_graph: Option<&'a ReactiveGraph>,
    ui_sender: Option<UiSender>,
    task_tracker: Option<TaskTracker>,
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
            message_queue: None,
            arena: None,
            reactive_graph: None,
            ui_sender: None,
            task_tracker: None,
            _marker: PhantomData,
        }
    }

    /// Creates a new [`ComponentContext`] bound to an invalidation cell and a [`MessageQueue`].
    #[inline]
    #[must_use]
    pub fn with_queue(
        node_id: NodeId,
        invalidated: &'a Cell<bool>,
        message_queue: MessageQueue,
    ) -> Self {
        Self {
            node_id,
            invalidated: Some(invalidated),
            message_queue: Some(message_queue),
            arena: None,
            reactive_graph: None,
            ui_sender: None,
            task_tracker: None,
            _marker: PhantomData,
        }
    }

    /// Creates a new [`ComponentContext`] bound to an invalidation cell and a runtime [`Arena`].
    #[inline]
    #[must_use]
    pub fn with_arena(node_id: NodeId, invalidated: &'a Cell<bool>, arena: &'a Arena) -> Self {
        Self {
            node_id,
            invalidated: Some(invalidated),
            message_queue: None,
            arena: Some(arena),
            reactive_graph: arena.reactive_graph.as_deref(),
            ui_sender: arena.ui_sender.clone(),
            task_tracker: arena.task_tracker.clone(),
            _marker: PhantomData,
        }
    }

    /// Creates a new [`ComponentContext`] bound to an invalidation cell, [`Arena`], and [`MessageQueue`].
    #[inline]
    #[must_use]
    pub fn with_arena_and_queue(
        node_id: NodeId,
        invalidated: &'a Cell<bool>,
        arena: &'a Arena,
        message_queue: MessageQueue,
    ) -> Self {
        Self {
            node_id,
            invalidated: Some(invalidated),
            message_queue: Some(message_queue),
            arena: Some(arena),
            reactive_graph: arena.reactive_graph.as_deref(),
            ui_sender: arena.ui_sender.clone(),
            task_tracker: arena.task_tracker.clone(),
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
            message_queue: None,
            arena: None,
            reactive_graph: None,
            ui_sender: None,
            task_tracker: None,
            _marker: PhantomData,
        }
    }

    /// Creates a [`ComponentContext`] bound to a runtime [`Arena`] without an invalidation cell.
    #[inline]
    #[must_use]
    pub fn for_arena(node_id: NodeId, arena: &'a Arena) -> Self {
        Self {
            node_id,
            invalidated: None,
            message_queue: None,
            arena: Some(arena),
            reactive_graph: arena.reactive_graph.as_deref(),
            ui_sender: arena.ui_sender.clone(),
            task_tracker: arena.task_tracker.clone(),
            _marker: PhantomData,
        }
    }

    /// Attaches an active [`ReactiveGraph`] to this execution context.
    #[inline]
    #[must_use]
    pub fn with_reactive_graph(mut self, reactive_graph: &'a ReactiveGraph) -> Self {
        self.reactive_graph = Some(reactive_graph);
        self
    }

    /// Attaches a [`UiSender`] to this execution context.
    #[inline]
    #[must_use]
    pub fn with_ui_sender(mut self, sender: UiSender) -> Self {
        self.ui_sender = Some(sender);
        self
    }

    /// Attaches a [`TaskTracker`] to this execution context.
    #[inline]
    #[must_use]
    pub fn with_task_tracker(mut self, tracker: TaskTracker) -> Self {
        self.task_tracker = Some(tracker);
        self
    }

    /// Returns the attached [`UiSender`], if any.
    #[inline]
    #[must_use]
    pub fn ui_sender(&self) -> Option<UiSender> {
        self.ui_sender.clone()
    }

    /// Returns the attached [`UiSender`], if any.
    #[inline]
    #[must_use]
    pub fn sender(&self) -> Option<UiSender> {
        self.ui_sender.clone()
    }

    /// Returns the attached [`ReactiveGraph`], if any.
    #[inline]
    #[must_use]
    pub fn reactive_graph(&self) -> Option<&'a ReactiveGraph> {
        self.reactive_graph
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
        if let Some(graph) = self.reactive_graph {
            graph.mark_node_dirty(self.node_id);
        }
    }

    /// Returns whether this context has been marked dirty for re-rendering.
    #[inline]
    #[must_use]
    pub fn is_invalidated(&self) -> bool {
        self.invalidated.is_some_and(|c| c.get())
    }

    /// Attaches a typed context value to this component's instance node.
    ///
    /// The context is accessible to this component and all its descendants via
    /// [`ComponentContext::use_context`] or [`RenderContext::use_context`] in $O(\text{depth})$.
    pub fn provide_context<T: Clone + 'static>(&self, value: T) {
        if let Some(arena) = self.arena {
            arena.provide_context(self.node_id, value);
        }
    }

    /// Retrieves a cloned context value of type `T` from this node or its nearest ancestor.
    ///
    /// Walks up the tree hierarchy in $O(\text{depth})$ and returns the first matching context
    /// value, supporting subtree shadowing. Returns `None` if no matching context is provided
    /// in the ancestor chain or if this context is not bound to a runtime arena.
    #[must_use]
    pub fn use_context<T: Clone + 'static>(&self) -> Option<T> {
        self.arena.and_then(|arena| arena.use_context(self.node_id))
    }

    /// Re-binds this execution context to another component type without changing the underlying state.
    #[inline]
    #[must_use]
    pub fn rebind<NewC: ?Sized>(&mut self) -> ComponentContext<'_, NewC> {
        ComponentContext {
            node_id: self.node_id,
            invalidated: self.invalidated,
            message_queue: self.message_queue.clone(),
            arena: self.arena,
            reactive_graph: self.reactive_graph,
            ui_sender: self.ui_sender.clone(),
            task_tracker: self.task_tracker.clone(),
            _marker: PhantomData,
        }
    }
}

impl<C: ?Sized> ReactiveContext for ComponentContext<'_, C> {
    #[inline]
    fn reactive_graph(&self) -> &ReactiveGraph {
        self.reactive_graph
            .expect("ComponentContext has no active ReactiveGraph bound")
    }
}

impl<C: Component> ComponentContext<'_, C> {
    /// Creates a callback that sends a message to this component's `update` method.
    ///
    /// The callback captures only the component's [`NodeId`] and a shared handle to the
    /// runtime's local message queue, avoiding lifetime contagion and allowing the callback
    /// to satisfy `'static` for component props (RFC-0001 §8.2).
    #[must_use]
    pub fn callback<In: 'static>(&self, f: impl Fn(In) -> C::Message + 'static) -> Callback<In> {
        let target = self.node_id;
        let queue = self.message_queue.clone();
        Callback::new(move |input: In| {
            if let Some(q) = &queue {
                let msg = f(input);
                q.push_typed(target, msg);
            }
        })
    }

    /// Spawns an asynchronous Tokio background task scoped to this component instance.
    ///
    /// The task is registered with the runtime's [`TaskTracker`]. If this component
    /// unmounts before the task completes, the task is automatically aborted.
    ///
    /// If the future produces an output type that is non-`()`, the result is automatically
    /// boxed and sent to this component's `update` method via [`UiSender`]. If the component
    /// has unmounted by the time the message arrives, it is safely dropped.
    pub fn spawn<F>(&mut self, future: F) -> tokio::task::JoinHandle<()>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let node_id = self.node_id;
        let ui_sender = self.ui_sender.clone();
        let tracker = self.task_tracker.clone();

        let handle = tokio::spawn(async move {
            let output = future.await;
            if TypeId::of::<F::Output>() != TypeId::of::<()>() {
                if let Some(sender) = ui_sender {
                    let boxed: Box<dyn std::any::Any + Send> = Box::new(output);
                    let _ = sender.send(node_id, boxed);
                }
            }
        });

        if let Some(t) = &tracker {
            t.register(node_id, handle.abort_handle());
        }

        handle
    }
}

impl<C: ?Sized> fmt::Debug for ComponentContext<'_, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentContext")
            .field("node_id", &self.node_id)
            .field("is_invalidated", &self.is_invalidated())
            .field("has_arena", &self.arena.is_some())
            .field("has_ui_sender", &self.ui_sender.is_some())
            .finish()
    }
}

/// Read-only rendering context passed to [`super::Component::render`].
///
/// Exposes the component's [`NodeId`], attached scoped contexts, and allows the component
/// or its children to schedule re-renders if necessary during layout or render evaluation.
pub struct RenderContext<'a, C: ?Sized> {
    node_id: NodeId,
    invalidated: Option<&'a Cell<bool>>,
    message_queue: Option<MessageQueue>,
    arena: Option<&'a Arena>,
    reactive_graph: Option<&'a ReactiveGraph>,
    ui_sender: Option<UiSender>,
    task_tracker: Option<TaskTracker>,
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
            message_queue: None,
            arena: None,
            reactive_graph: None,
            ui_sender: None,
            task_tracker: None,
            _marker: PhantomData,
        }
    }

    /// Creates a new [`RenderContext`] bound to an invalidation cell and a [`MessageQueue`].
    #[inline]
    #[must_use]
    pub fn with_queue(
        node_id: NodeId,
        invalidated: &'a Cell<bool>,
        message_queue: MessageQueue,
    ) -> Self {
        Self {
            node_id,
            invalidated: Some(invalidated),
            message_queue: Some(message_queue),
            arena: None,
            reactive_graph: None,
            ui_sender: None,
            task_tracker: None,
            _marker: PhantomData,
        }
    }

    /// Creates a new [`RenderContext`] bound to an invalidation cell and a runtime [`Arena`].
    #[inline]
    #[must_use]
    pub fn with_arena(node_id: NodeId, invalidated: &'a Cell<bool>, arena: &'a Arena) -> Self {
        Self {
            node_id,
            invalidated: Some(invalidated),
            message_queue: None,
            arena: Some(arena),
            reactive_graph: arena.reactive_graph.as_deref(),
            ui_sender: arena.ui_sender.clone(),
            task_tracker: arena.task_tracker.clone(),
            _marker: PhantomData,
        }
    }

    /// Creates a new [`RenderContext`] bound to an invalidation cell, [`Arena`], and [`MessageQueue`].
    #[inline]
    #[must_use]
    pub fn with_arena_and_queue(
        node_id: NodeId,
        invalidated: &'a Cell<bool>,
        arena: &'a Arena,
        message_queue: MessageQueue,
    ) -> Self {
        Self {
            node_id,
            invalidated: Some(invalidated),
            message_queue: Some(message_queue),
            arena: Some(arena),
            reactive_graph: arena.reactive_graph.as_deref(),
            ui_sender: arena.ui_sender.clone(),
            task_tracker: arena.task_tracker.clone(),
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
            message_queue: None,
            arena: None,
            reactive_graph: None,
            ui_sender: None,
            task_tracker: None,
            _marker: PhantomData,
        }
    }

    /// Creates a [`RenderContext`] bound to a runtime [`Arena`] without an invalidation cell.
    #[inline]
    #[must_use]
    pub fn for_arena(node_id: NodeId, arena: &'a Arena) -> Self {
        Self {
            node_id,
            invalidated: None,
            message_queue: None,
            arena: Some(arena),
            reactive_graph: arena.reactive_graph.as_deref(),
            ui_sender: arena.ui_sender.clone(),
            task_tracker: arena.task_tracker.clone(),
            _marker: PhantomData,
        }
    }

    /// Attaches an active [`ReactiveGraph`] to this rendering context.
    #[inline]
    #[must_use]
    pub fn with_reactive_graph(mut self, reactive_graph: &'a ReactiveGraph) -> Self {
        self.reactive_graph = Some(reactive_graph);
        self
    }

    /// Attaches a [`UiSender`] to this rendering context.
    #[inline]
    #[must_use]
    pub fn with_ui_sender(mut self, sender: UiSender) -> Self {
        self.ui_sender = Some(sender);
        self
    }

    /// Attaches a [`TaskTracker`] to this rendering context.
    #[inline]
    #[must_use]
    pub fn with_task_tracker(mut self, tracker: TaskTracker) -> Self {
        self.task_tracker = Some(tracker);
        self
    }

    /// Returns the attached [`UiSender`], if any.
    #[inline]
    #[must_use]
    pub fn ui_sender(&self) -> Option<UiSender> {
        self.ui_sender.clone()
    }

    /// Returns the attached [`UiSender`], if any.
    #[inline]
    #[must_use]
    pub fn sender(&self) -> Option<UiSender> {
        self.ui_sender.clone()
    }

    /// Returns the attached [`ReactiveGraph`], if any.
    #[inline]
    #[must_use]
    pub fn reactive_graph(&self) -> Option<&'a ReactiveGraph> {
        self.reactive_graph
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
        if let Some(graph) = self.reactive_graph {
            graph.mark_node_dirty(self.node_id);
        }
    }

    /// Returns whether this context has been marked dirty for re-rendering.
    #[inline]
    #[must_use]
    pub fn is_invalidated(&self) -> bool {
        self.invalidated.is_some_and(|c| c.get())
    }

    /// Attaches a typed context value to this component's instance node.
    ///
    /// The context is accessible to this component and all its descendants via
    /// [`ComponentContext::use_context`] or [`RenderContext::use_context`] in $O(\text{depth})$.
    pub fn provide_context<T: Clone + 'static>(&self, value: T) {
        if let Some(arena) = self.arena {
            arena.provide_context(self.node_id, value);
        }
    }

    /// Retrieves a cloned context value of type `T` from this node or its nearest ancestor.
    ///
    /// Walks up the tree hierarchy in $O(\text{depth})$ and returns the first matching context
    /// value, supporting subtree shadowing. Returns `None` if no matching context is provided
    /// in the ancestor chain or if this context is not bound to a runtime arena.
    #[must_use]
    pub fn use_context<T: Clone + 'static>(&self) -> Option<T> {
        self.arena.and_then(|arena| arena.use_context(self.node_id))
    }

    /// Re-binds this render context to another component type without changing the underlying state.
    #[inline]
    #[must_use]
    pub fn rebind<NewC: ?Sized>(&self) -> RenderContext<'_, NewC> {
        RenderContext {
            node_id: self.node_id,
            invalidated: self.invalidated,
            message_queue: self.message_queue.clone(),
            arena: self.arena,
            reactive_graph: self.reactive_graph,
            ui_sender: self.ui_sender.clone(),
            task_tracker: self.task_tracker.clone(),
            _marker: PhantomData,
        }
    }
}

impl<C: ?Sized> ReactiveContext for RenderContext<'_, C> {
    #[inline]
    fn reactive_graph(&self) -> &ReactiveGraph {
        self.reactive_graph
            .expect("RenderContext has no active ReactiveGraph bound")
    }
}

impl<C: Component> RenderContext<'_, C> {
    /// Creates a callback that sends a message to this component's `update` method.
    ///
    /// Useful for passing event callbacks to child components created during `render`.
    #[must_use]
    pub fn callback<In: 'static>(&self, f: impl Fn(In) -> C::Message + 'static) -> Callback<In> {
        let target = self.node_id;
        let queue = self.message_queue.clone();
        Callback::new(move |input: In| {
            if let Some(q) = &queue {
                let msg = f(input);
                q.push_typed(target, msg);
            }
        })
    }

    /// Spawns an asynchronous Tokio background task scoped to this component instance.
    pub fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<()>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let node_id = self.node_id;
        let ui_sender = self.ui_sender.clone();
        let tracker = self.task_tracker.clone();

        let handle = tokio::spawn(async move {
            let output = future.await;
            if TypeId::of::<F::Output>() != TypeId::of::<()>() {
                if let Some(sender) = ui_sender {
                    let boxed: Box<dyn std::any::Any + Send> = Box::new(output);
                    let _ = sender.send(node_id, boxed);
                }
            }
        });

        if let Some(t) = &tracker {
            t.register(node_id, handle.abort_handle());
        }

        handle
    }
}

impl<C: ?Sized> fmt::Debug for RenderContext<'_, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderContext")
            .field("node_id", &self.node_id)
            .field("is_invalidated", &self.is_invalidated())
            .field("has_arena", &self.arena.is_some())
            .field("has_ui_sender", &self.ui_sender.is_some())
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

    #[test]
    fn context_provisioning_and_lookup() {
        use crate::id::NodeKey;
        use crate::runtime::arena::InstanceNode;

        #[derive(Clone, PartialEq, Debug)]
        struct MyTheme(String);

        let mut arena = Arena::new();
        let root_id = arena.allocate(|id| {
            InstanceNode::placeholder(
                id,
                None,
                0,
                NodeKey::Positional { scope: 0, index: 0 },
                std::any::TypeId::of::<Dummy>(),
            )
        });
        let child_id = arena.allocate(|id| {
            InstanceNode::placeholder(
                id,
                Some(root_id),
                1,
                NodeKey::Positional { scope: 0, index: 1 },
                std::any::TypeId::of::<Dummy>(),
            )
        });

        let root_ctx = ComponentContext::<Dummy>::for_arena(root_id, &arena);
        root_ctx.provide_context(MyTheme("nord".to_string()));

        let child_ctx = ComponentContext::<Dummy>::for_arena(child_id, &arena);
        assert_eq!(
            child_ctx.use_context::<MyTheme>(),
            Some(MyTheme("nord".to_string()))
        );

        let child_render = RenderContext::<Dummy>::for_arena(child_id, &arena);
        assert_eq!(
            child_render.use_context::<MyTheme>(),
            Some(MyTheme("nord".to_string()))
        );
        assert_eq!(child_render.use_context::<i32>(), None);
    }
}
