//! Primary runtime container coordinating arena storage, reactivity, messaging, and scheduling.
//!
//! Implements the frame lifecycle and batch mutation execution specified in **RFC-0001 §4.2 and §7.2**.

use super::arena::Arena;
use super::flags::NodeFlags;
use super::message::MessageQueue;
use super::reconcile::mount_node;
use super::scheduler::Scheduler;
use crate::id::NodeId;
use crate::node::component::ComponentDescriptor;
use crate::reactivity::{ReactiveContext, ReactiveGraph};

use std::cell::Cell;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

/// The primary Nyrui UI runtime managing components, reactivity, messaging, and scheduling.
pub struct Runtime {
    /// Generational arena holding mounted component instances.
    pub arena: Arena,
    /// Shared reactive graph managing signals and derived computations.
    pub reactive_graph: Rc<ReactiveGraph>,
    /// Asynchronous action and event message queue.
    pub message_queue: MessageQueue,
    /// Min-heap depth-ordered dirty node scheduler.
    pub scheduler: Scheduler,
    /// Invalidation flag cell indicating pending UI reconciliation passes.
    pub dirty_cell: Cell<bool>,
}

impl Default for Runtime {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// Creates a new [`Runtime`] with a fresh [`ReactiveGraph`], [`Arena`], [`MessageQueue`], and [`Scheduler`].
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        let reactive_graph = Rc::new(ReactiveGraph::new());
        let arena = Arena::new().with_reactive_graph(reactive_graph.clone());
        let message_queue = MessageQueue::new();
        let scheduler = Scheduler::new();
        let dirty_cell = Cell::new(false);

        Self {
            arena,
            reactive_graph,
            message_queue,
            scheduler,
            dirty_cell,
        }
    }

    /// Creates a new [`Runtime`] with pre-allocated slot and scheduler capacity.
    #[inline]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let reactive_graph = Rc::new(ReactiveGraph::with_capacity(capacity));
        let arena = Arena::with_capacity(capacity).with_reactive_graph(reactive_graph.clone());
        let message_queue = MessageQueue::new();
        let scheduler = Scheduler::with_capacity(capacity);
        let dirty_cell = Cell::new(false);

        Self {
            arena,
            reactive_graph,
            message_queue,
            scheduler,
            dirty_cell,
        }
    }

    /// Mounts a root component into the arena at depth 0, executing initial lifecycle and render passes.
    ///
    /// Returns the allocated [`NodeId`] of the mounted root instance.
    pub fn mount_root(&mut self, descriptor: ComponentDescriptor) -> NodeId {
        mount_node(&mut self.arena, None, 0, descriptor, &self.dirty_cell)
    }

    /// Drains pending action messages and flushes all dirty nodes in top-down depth order.
    ///
    /// Returns the total number of components rendered during this flush pass.
    pub fn flush(&mut self) -> usize {
        let _ = self
            .message_queue
            .process_messages(&mut self.arena, &self.dirty_cell);
        self.scheduler
            .flush(&mut self.arena, &self.reactive_graph, &self.dirty_cell)
    }

    /// Returns `true` if there are pending dirty nodes, messages, or active invalidations.
    #[inline]
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty_cell.get()
            || !self.scheduler.is_empty()
            || self.reactive_graph.has_dirty_nodes()
            || !self.message_queue.is_empty()
    }

    /// Marks a specific node as dirty and enqueues it into the scheduler.
    pub fn invalidate(&mut self, id: NodeId) {
        if let Some(node) = self.arena.get_mut(id) {
            node.flags.insert(NodeFlags::DIRTY);
            self.scheduler.enqueue(&self.arena, id);
            self.dirty_cell.set(true);
        }
    }

    /// Sends an action message to a target component instance in the arena.
    #[inline]
    pub fn send_message<M: 'static>(&self, target: NodeId, msg: M) {
        self.message_queue.push_typed(target, msg);
    }

    /// Executes a batch closure grouping multiple state/signal mutations, then runs
    /// a single depth-ordered flush pass.
    ///
    /// # Example
    ///
    /// ```rust
    /// use nyrui_core::runtime::Runtime;
    ///
    /// let mut runtime = Runtime::new();
    /// let sig = runtime.reactive_graph.create_signal(10);
    ///
    /// runtime.batch(|cx| {
    ///     sig.set(cx, 20);
    ///     sig.set(cx, 30);
    /// });
    /// ```
    pub fn batch<R>(&mut self, f: impl FnOnce(&mut RuntimeBatchContext<'_>) -> R) -> R {
        let mut ctx = RuntimeBatchContext { runtime: self };
        let result = f(&mut ctx);
        self.flush();
        result
    }
}

impl ReactiveContext for Runtime {
    #[inline]
    fn reactive_graph(&self) -> &ReactiveGraph {
        &self.reactive_graph
    }
}

impl fmt::Debug for Runtime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runtime")
            .field("arena_len", &self.arena.len())
            .field("scheduler_len", &self.scheduler.len())
            .field("message_queue_len", &self.message_queue.len())
            .field("dirty", &self.is_dirty())
            .finish()
    }
}

/// Execution context passed to [`Runtime::batch`] providing signal access and component dispatch.
#[derive(Debug)]
pub struct RuntimeBatchContext<'a> {
    runtime: &'a mut Runtime,
}

impl Deref for RuntimeBatchContext<'_> {
    type Target = Runtime;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.runtime
    }
}

impl DerefMut for RuntimeBatchContext<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.runtime
    }
}

impl ReactiveContext for RuntimeBatchContext<'_> {
    #[inline]
    fn reactive_graph(&self) -> &ReactiveGraph {
        &self.runtime.reactive_graph
    }
}

impl RuntimeBatchContext<'_> {
    /// Invalidates a target node, marking it dirty and scheduling it in the min-heap.
    #[inline]
    pub fn invalidate(&mut self, id: NodeId) {
        self.runtime.invalidate(id);
    }

    /// Sends an action message to a target component instance in the arena.
    #[inline]
    pub fn send_message<M: 'static>(&self, target: NodeId, msg: M) {
        self.runtime.send_message(target, msg);
    }
}
