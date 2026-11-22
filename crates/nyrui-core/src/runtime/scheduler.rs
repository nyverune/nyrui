//! Depth-ordered dirty node scheduler and batch coalescing engine.
//!
//! Implements the frame lifecycle and dirty queue specification from **RFC-0001 §4.2 and §7.2**:
//! - Top-down min-heap ordering by tree `depth` ($O(\log N)$ enqueue and pop).
//! - Deduplication via [`HashSet<NodeId>`] to avoid duplicate renders in the same batch.
//! - Redundant render elimination: skips unmounted nodes or nodes already reconciled/cleared by ancestors.
//! - Batch mutation coalescing via [`Scheduler::batch`] or [`super::Runtime::batch`].

use crate::id::NodeId;
use crate::reactivity::ReactiveGraph;
use crate::runtime::arena::{Arena, NodeFlags};
use crate::runtime::reconcile::reconcile_children;

use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

/// Maximum flush passes allowed before cycle detection triggers.
pub const DEFAULT_MAX_FLUSH_PASSES: usize = 100_000;

/// An enqueued node entry prioritized by tree depth in the scheduler min-heap.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ScheduledNode {
    /// Depth of the node in the arena tree (root = 0).
    pub depth: u16,
    /// Generational identifier of the target node.
    pub id: NodeId,
}

impl Ord for ScheduledNode {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap ordering: smaller depth has higher priority (popped first).
        // Tie-breaker: smaller NodeId has higher priority for deterministic ordering.
        other
            .depth
            .cmp(&self.depth)
            .then_with(|| other.id.cmp(&self.id))
    }
}

impl PartialOrd for ScheduledNode {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Min-heap depth-ordered scheduler coordinating component re-rendering and batch coalescing.
#[derive(Clone, Debug)]
pub struct Scheduler {
    queue: BinaryHeap<ScheduledNode>,
    enqueued: HashSet<NodeId>,
    max_passes: usize,
}

impl Default for Scheduler {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    /// Creates a new empty [`Scheduler`].
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            enqueued: HashSet::new(),
            max_passes: DEFAULT_MAX_FLUSH_PASSES,
        }
    }

    /// Creates a new empty [`Scheduler`] with pre-allocated capacity.
    #[inline]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            queue: BinaryHeap::with_capacity(capacity),
            enqueued: HashSet::with_capacity(capacity),
            max_passes: DEFAULT_MAX_FLUSH_PASSES,
        }
    }

    /// Sets the maximum allowable flush iterations before detecting an infinite loop.
    pub fn set_max_passes(&mut self, max: usize) {
        self.max_passes = max;
    }

    /// Returns the maximum allowable flush iterations.
    #[inline]
    #[must_use]
    pub fn max_passes(&self) -> usize {
        self.max_passes
    }

    /// Returns the number of nodes currently pending in the schedule.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns `true` if there are no nodes pending in the schedule.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns `true` if the node is currently pending in the schedule.
    #[inline]
    #[must_use]
    pub fn contains(&self, id: NodeId) -> bool {
        self.enqueued.contains(&id)
    }

    /// Clears all pending nodes from the scheduler.
    #[inline]
    pub fn clear(&mut self) {
        self.queue.clear();
        self.enqueued.clear();
    }

    /// Enqueues a node for reconciliation, looking up its tree depth from the arena.
    ///
    /// Returns `true` if the node was newly added, or `false` if it was already enqueued
    /// or not found in the arena.
    pub fn enqueue(&mut self, arena: &Arena, id: NodeId) -> bool {
        if !arena.contains(id) {
            return false;
        }
        if self.enqueued.insert(id) {
            let depth = arena.get(id).map_or(0, |n| n.depth);
            self.queue.push(ScheduledNode { depth, id });
            true
        } else {
            false
        }
    }

    /// Enqueues a node with an explicitly known tree depth.
    ///
    /// Deduplicates entries so a node is enqueued at most once per flush.
    pub fn enqueue_with_depth(&mut self, id: NodeId, depth: u16) -> bool {
        if self.enqueued.insert(id) {
            self.queue.push(ScheduledNode { depth, id });
            true
        } else {
            false
        }
    }

    /// Drains all dirty nodes from the [`ReactiveGraph`], marks their arena flags as
    /// [`NodeFlags::DIRTY`], and pushes them into the scheduler.
    ///
    /// Returns the number of newly scheduled nodes.
    pub fn enqueue_dirty_nodes(&mut self, arena: &mut Arena, graph: &ReactiveGraph) -> usize {
        let dirty_ids = graph.take_dirty_nodes();
        let mut count = 0;
        for id in dirty_ids {
            if let Some(node) = arena.get_mut(id) {
                node.flags.insert(NodeFlags::DIRTY);
                let depth = node.depth;
                if self.enqueued.insert(id) {
                    self.queue.push(ScheduledNode { depth, id });
                    count += 1;
                }
            }
        }
        count
    }

    /// Pops the next highest priority node (shallowest tree depth) from the queue.
    pub fn pop(&mut self) -> Option<ScheduledNode> {
        let item = self.queue.pop()?;
        self.enqueued.remove(&item.id);
        Some(item)
    }

    /// Flushes all pending dirty nodes in top-down depth order.
    ///
    /// For each node:
    /// 1. Skips unmounted instances (e.g. unmounted by an ancestor during this flush pass).
    /// 2. Skips instances whose [`NodeFlags::DIRTY`] has been cleared or is not set.
    /// 3. Clears [`NodeFlags::DIRTY`].
    /// 4. Executes [`Arena::render`] to re-evaluate the component.
    /// 5. Reconciles children via [`reconcile_children`].
    /// 6. Recursively enqueues any children or reactive subscribers dirtied during the render.
    ///
    /// Returns the total number of components actually rendered.
    ///
    /// # Panics
    ///
    /// Panics if total render passes exceed [`Self::max_passes`], indicating an infinite loop
    /// or cycle.
    pub fn flush(
        &mut self,
        arena: &mut Arena,
        graph: &ReactiveGraph,
        dirty_cell: &Cell<bool>,
    ) -> usize {
        self.enqueue_dirty_nodes(arena, graph);

        let mut rendered_count = 0;
        let mut iterations = 0;

        while let Some(ScheduledNode { depth: _, id }) = self.pop() {
            iterations += 1;
            if iterations > self.max_passes {
                panic!(
                    "Exceeded maximum scheduler flush iterations ({}); possible cyclic re-render loop detected",
                    self.max_passes
                );
            }

            // 1. Skip unmounted nodes (e.g. removed when an ancestor reconciled)
            if !arena.contains(id) {
                continue;
            }

            // 2. Skip nodes that are no longer dirty
            let is_dirty = arena
                .get(id)
                .is_some_and(|node| node.flags.contains(NodeFlags::DIRTY));
            if !is_dirty {
                continue;
            }

            // 3. Clear DIRTY flag on current node
            if let Some(node) = arena.get_mut(id) {
                node.flags.remove(NodeFlags::DIRTY);
            }

            // 4. Render node
            let new_vdom = match arena.render(id, dirty_cell) {
                Ok(vdom) => vdom,
                Err(_) => continue,
            };
            rendered_count += 1;

            // 5. Lookup children and depth for reconciliation
            let (old_children, current_depth) = match arena.get(id) {
                Some(node) => (node.children.clone(), node.depth),
                None => continue,
            };

            // 6. Reconcile children
            let recon = reconcile_children(
                arena,
                Some(id),
                current_depth + 1,
                &old_children,
                vec![new_vdom],
                dirty_cell,
            );

            // 7. For newly mounted children, clear initial DIRTY flag since their initial render is complete
            for &new_id in &recon.newly_mounted {
                if let Some(new_node) = arena.get_mut(new_id) {
                    new_node.flags.remove(NodeFlags::DIRTY);
                }
            }

            // 8. Enqueue any matched children marked dirty during property updates
            for &child_id in &recon.children {
                if !recon.newly_mounted.contains(&child_id) {
                    if let Some(child_node) = arena.get(child_id) {
                        if child_node.flags.contains(NodeFlags::DIRTY) {
                            self.enqueue(arena, child_id);
                        }
                    }
                }
            }

            // 9. Drain any newly dirtied nodes from signals mutated during render
            self.enqueue_dirty_nodes(arena, graph);
        }

        if self.is_empty() {
            dirty_cell.set(false);
        }

        rendered_count
    }

    /// Executes a closure that performs multiple state or signal mutations, then
    /// runs a single depth-ordered flush pass.
    ///
    /// Returns the closure result and the number of rendered components.
    pub fn batch<R>(
        &mut self,
        arena: &mut Arena,
        graph: &ReactiveGraph,
        dirty_cell: &Cell<bool>,
        f: impl FnOnce(&ReactiveGraph) -> R,
    ) -> (R, usize) {
        let result = f(graph);
        let rendered = self.flush(arena, graph, dirty_cell);
        (result, rendered)
    }
}
