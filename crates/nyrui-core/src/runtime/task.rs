//! Scoped background task tracking and cancellation.
//!
//! Enforces the lifecycle guarantee from **RFC-0001 §3.1 and §13.1**:
//! when a component instance unmounts, all active background tasks spawned
//! by it are immediately aborted via [`tokio::task::AbortHandle`].

use crate::id::NodeId;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::task::AbortHandle;

/// Manages active Tokio background tasks scoped to arena component nodes.
#[derive(Clone, Default, Debug)]
pub struct TaskTracker {
    tasks: Rc<RefCell<HashMap<NodeId, Vec<AbortHandle>>>>,
}

impl TaskTracker {
    /// Creates a new empty [`TaskTracker`].
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            tasks: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Registers an active task abort handle under the specified component [`NodeId`].
    pub fn register(&self, id: NodeId, handle: AbortHandle) {
        self.tasks.borrow_mut().entry(id).or_default().push(handle);
    }

    /// Cancels and aborts all active tasks attached to the given node ID.
    ///
    /// Returns the number of aborted tasks.
    pub fn cancel_node_tasks(&self, id: NodeId) -> usize {
        let mut map = self.tasks.borrow_mut();
        if let Some(handles) = map.remove(&id) {
            let count = handles.len();
            for handle in handles {
                handle.abort();
            }
            count
        } else {
            0
        }
    }

    /// Returns the number of active tasks currently tracked for the given node.
    #[must_use]
    pub fn task_count(&self, id: NodeId) -> usize {
        self.tasks.borrow().get(&id).map_or(0, |v| v.len())
    }

    /// Returns the total number of active tasks tracked across all nodes.
    #[must_use]
    pub fn total_tasks(&self) -> usize {
        self.tasks.borrow().values().map(Vec::len).sum()
    }

    /// Aborts all tracked tasks across all component nodes.
    pub fn cancel_all(&self) {
        let mut map = self.tasks.borrow_mut();
        for (_, handles) in map.drain() {
            for handle in handles {
                handle.abort();
            }
        }
    }
}
