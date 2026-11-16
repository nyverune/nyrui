//! Take-and-Restore execution helpers for safe component lifecycle dispatch.

use super::error::ArenaError;
use super::storage::Arena;
use crate::component::{ComponentContext, RenderContext, ShouldRender};
use crate::id::NodeId;
use crate::node::UiNode;
use crate::runtime::erased::ErasedComponent;

use std::any::Any;
use std::cell::Cell;

impl Arena {
    /// Executes a closure with exclusive access to a component instance, temporarily
    /// taking it out of its arena slot and restoring it immediately upon completion.
    ///
    /// This pattern (RFC-0001 §6.2) enables the closure to access the rest of the [`Arena`]
    /// (e.g. for re-entrant child inspection, parent updates, or tree queries) without
    /// aliasing borrow conflicts, `RefCell` panics, or `unsafe` blocks.
    pub fn with_instance_mut<R>(
        &mut self,
        id: NodeId,
        f: impl FnOnce(&mut Box<dyn ErasedComponent>, &mut Self) -> R,
    ) -> Result<R, ArenaError> {
        let mut instance = match self.get_mut(id) {
            Some(node) => match node.instance.take() {
                Some(inst) => inst,
                None => return Err(ArenaError::ComponentAlreadyBorrowed(id)),
            },
            None => return Err(ArenaError::NodeNotFound(id)),
        };

        // Execute closure with both &mut instance and &mut self accessible
        let result = f(&mut instance, self);

        // Restore instance back into the slot
        if let Some(node) = self.get_mut(id) {
            node.instance = Some(instance);
        }

        Ok(result)
    }

    /// Read-only variant of the Take-and-Restore pattern.
    pub fn with_instance<R>(
        &mut self,
        id: NodeId,
        f: impl FnOnce(&Box<dyn ErasedComponent>, &mut Self) -> R,
    ) -> Result<R, ArenaError> {
        self.with_instance_mut(id, |inst, arena| f(&*inst, arena))
    }

    /// Updates an instance's properties using the Take-and-Restore pattern.
    pub fn update_props(
        &mut self,
        id: NodeId,
        new_props: Box<dyn Any>,
        invalidated: &Cell<bool>,
    ) -> Result<ShouldRender, ArenaError> {
        self.with_instance_mut(id, |instance, arena| {
            let mut ctx = ComponentContext::with_arena(id, invalidated, arena);
            match instance.update_props_erased(new_props, &mut ctx) {
                Ok(decision) => Ok(decision),
                Err(_) => Err(ArenaError::DowncastFailed),
            }
        })?
    }

    /// Dispatches an action message to an instance using the Take-and-Restore pattern.
    pub fn update(
        &mut self,
        id: NodeId,
        msg: Box<dyn Any>,
        invalidated: &Cell<bool>,
    ) -> Result<ShouldRender, ArenaError> {
        self.with_instance_mut(id, |instance, arena| {
            let mut ctx = ComponentContext::with_arena(id, invalidated, arena);
            match instance.update_erased(msg, &mut ctx) {
                Ok(decision) => Ok(decision),
                Err(_) => Err(ArenaError::DowncastFailed),
            }
        })?
    }

    /// Renders an instance's visual subtree using the Take-and-Restore pattern.
    pub fn render(&mut self, id: NodeId, invalidated: &Cell<bool>) -> Result<UiNode, ArenaError> {
        self.with_instance(id, |instance, arena| {
            let ctx = RenderContext::with_arena(id, invalidated, arena);
            instance.render_erased(&ctx)
        })
    }

    /// Invokes the `on_mount` lifecycle hook using Take-and-Restore.
    pub fn on_mount(&mut self, id: NodeId, invalidated: &Cell<bool>) -> Result<(), ArenaError> {
        self.with_instance_mut(id, |instance, arena| {
            let mut ctx = ComponentContext::with_arena(id, invalidated, arena);
            instance.on_mount(&mut ctx);
        })
    }

    /// Invokes the `on_unmount` lifecycle hook using Take-and-Restore.
    pub fn on_unmount(&mut self, id: NodeId, invalidated: &Cell<bool>) -> Result<(), ArenaError> {
        self.with_instance_mut(id, |instance, arena| {
            let mut ctx = ComponentContext::with_arena(id, invalidated, arena);
            instance.on_unmount(&mut ctx);
        })
    }

    /// Drains and processes all pending messages from the queue, dispatching them
    /// to active components and safely discarding messages targeting unmounted nodes.
    pub fn process_message_queue(
        &mut self,
        queue: &crate::runtime::message::MessageQueue,
        dirty_cell: &Cell<bool>,
    ) -> usize {
        queue.process_messages(self, dirty_cell)
    }
}
