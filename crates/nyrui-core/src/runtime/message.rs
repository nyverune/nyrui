//! Runtime message queue for asynchronous event and callback dispatch.

use crate::id::NodeId;
use crate::runtime::arena::{Arena, NodeFlags};

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fmt;
use std::rc::Rc;

/// A type-erased message targeted to a specific component instance in the arena.
pub struct QueuedMessage {
    /// The generational identifier of the target component.
    pub target: NodeId,
    /// Boxed, type-erased component-local message.
    pub message: Box<dyn Any>,
}

impl fmt::Debug for QueuedMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueuedMessage")
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}

/// Single-threaded FIFO message queue coordinating component action dispatch.
///
/// When callbacks fire, they push messages into this queue along with the target [`NodeId`].
/// During frame reconciliation or event processing, the queue is drained and dispatched
/// to the [`Arena`]. Stale messages targeting unmounted nodes are cleanly discarded
/// without panicking.
#[derive(Clone, Default)]
pub struct MessageQueue {
    queue: Rc<RefCell<VecDeque<QueuedMessage>>>,
}

impl MessageQueue {
    /// Creates a new empty [`MessageQueue`].
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    /// Pushes a pre-boxed message targeting the specified node.
    pub fn push(&self, target: NodeId, message: Box<dyn Any>) {
        self.queue
            .borrow_mut()
            .push_back(QueuedMessage { target, message });
    }

    /// Boxes and pushes a typed message targeting the specified node.
    #[inline]
    pub fn push_typed<M: 'static>(&self, target: NodeId, message: M) {
        self.push(target, Box::new(message));
    }

    /// Pops the front message from the queue, if any.
    #[must_use]
    pub fn pop_front(&self) -> Option<QueuedMessage> {
        self.queue.borrow_mut().pop_front()
    }

    /// Returns the number of messages currently waiting in the queue.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.borrow().len()
    }

    /// Returns `true` if the queue is empty.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.borrow().is_empty()
    }

    /// Clears all pending messages from the queue.
    #[inline]
    pub fn clear(&self) {
        self.queue.borrow_mut().clear();
    }

    /// Drains all pending messages into a `Vec<QueuedMessage>`.
    #[must_use]
    pub fn drain(&self) -> Vec<QueuedMessage> {
        self.queue.borrow_mut().drain(..).collect()
    }

    /// Drains and processes all pending messages from the queue, dispatching them
    /// to their target components in the [`Arena`].
    ///
    /// # Generational Safety
    ///
    /// If a target node has been unmounted or replaced with a new generation,
    /// [`Arena::contains`] returns `false`, causing the stale message to be
    /// silently discarded without panicking.
    ///
    /// Returns the number of successfully processed messages.
    pub fn process_messages(&self, arena: &mut Arena, dirty_cell: &Cell<bool>) -> usize {
        let messages = self.drain();
        let mut processed = 0;

        for msg in messages {
            // Safe generational check: discard if node unmounted or generation mismatched
            if arena.contains(msg.target) {
                if let Ok(should_render) = arena.update(msg.target, msg.message, dirty_cell) {
                    if should_render.is_yes() {
                        if let Some(node) = arena.get_mut(msg.target) {
                            node.flags.insert(NodeFlags::DIRTY);
                        }
                    }
                    processed += 1;
                }
            }
        }

        processed
    }
}

impl fmt::Debug for MessageQueue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MessageQueue")
            .field("len", &self.len())
            .finish()
    }
}
