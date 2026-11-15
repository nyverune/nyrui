//! Generational instance arena with Take-and-Restore borrowing.
//!
//! Implements the heterogeneous runtime storage specified in **RFC-0001 §6.2**.

pub use super::erased::{ComponentInstance, ErasedComponent};
pub use super::flags::NodeFlags;

use crate::component::{Component, ComponentContext, RenderContext, ShouldRender};
use crate::id::{NodeId, NodeKey};
use crate::node::UiNode;

use std::any::{Any, TypeId};
use std::cell::Cell;
use std::fmt;
use std::num::NonZeroU32;

/// A mounted component node stored inside the generational arena.
///
/// Contains tree topology, layout geometry cache, lifecycle flags, and the
/// type-erased component instance.
pub struct InstanceNode {
    /// Generational identifier of this instance in the arena.
    pub id: NodeId,
    /// Parent node identifier, or `None` if this is a root component.
    pub parent: Option<NodeId>,
    /// Ordered list of active child node identifiers.
    pub children: Vec<NodeId>,
    /// Unique [`TypeId`] of the underlying component.
    pub type_id: TypeId,
    /// Reconciliation key (explicit or positional).
    pub key: NodeKey,
    /// Tree depth from the root (root has depth 0).
    pub depth: u16,
    /// Status, focus, and dirty flags.
    pub flags: NodeFlags,
    /// Type-erased component instance.
    ///
    /// Wrapped in an [`Option`] to support the Take-and-Restore pattern (RFC-0001 §6.2)
    /// where the instance is temporarily removed during lifecycle/update calls.
    pub instance: Option<Box<dyn ErasedComponent>>,
}

impl InstanceNode {
    /// Creates a new [`InstanceNode`] from a concrete [`Component`] and its [`Component::Props`].
    #[inline]
    #[must_use]
    pub fn new<C: Component>(
        id: NodeId,
        parent: Option<NodeId>,
        depth: u16,
        key: NodeKey,
        component: C,
        props: C::Props,
    ) -> Self {
        Self {
            id,
            parent,
            children: Vec::new(),
            type_id: TypeId::of::<C>(),
            key,
            depth,
            flags: NodeFlags::DIRTY,
            instance: Some(Box::new(ComponentInstance::new(component, props))),
        }
    }

    /// Creates a new [`InstanceNode`] from a pre-boxed [`ErasedComponent`].
    #[inline]
    #[must_use]
    pub fn from_erased(
        id: NodeId,
        parent: Option<NodeId>,
        depth: u16,
        key: NodeKey,
        type_id: TypeId,
        instance: Box<dyn ErasedComponent>,
    ) -> Self {
        Self {
            id,
            parent,
            children: Vec::new(),
            type_id,
            key,
            depth,
            flags: NodeFlags::DIRTY,
            instance: Some(instance),
        }
    }
}

impl fmt::Debug for InstanceNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InstanceNode")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .field("children", &self.children)
            .field("type_id", &self.type_id)
            .field("key", &self.key)
            .field("depth", &self.depth)
            .field("flags", &self.flags)
            .field("has_instance", &self.instance.is_some())
            .finish()
    }
}

/// Slot in the generational arena storage.
#[derive(Debug)]
enum ArenaSlot {
    Occupied {
        generation: NonZeroU32,
        node: InstanceNode,
    },
    Vacant {
        generation: NonZeroU32,
        next_free: Option<u32>,
    },
}

/// Errors returned by arena operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArenaError {
    /// The specified node was not found or the generation is stale.
    NodeNotFound(NodeId),
    /// The component instance is currently taken out by another active invocation.
    ComponentAlreadyBorrowed(NodeId),
    /// Properties or message downcasting failed due to a type mismatch.
    DowncastFailed,
}

impl fmt::Display for ArenaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeNotFound(id) => {
                write!(f, "Node {id:?} not found in arena (stale or removed)")
            }
            Self::ComponentAlreadyBorrowed(id) => {
                write!(f, "Component {id:?} is already currently borrowed")
            }
            Self::DowncastFailed => write!(f, "Type mismatch during component downcast"),
        }
    }
}

impl std::error::Error for ArenaError {}

/// Single-threaded generational arena storing heterogeneous component instances.
///
/// Implements $O(1)$ allocation, $O(1)$ lookup, and $O(1)$ removal with ABA
/// generational protection (RFC-0001 §6.2).
#[derive(Debug, Default)]
pub struct Arena {
    slots: Vec<ArenaSlot>,
    free_head: Option<u32>,
    len: usize,
}

impl Arena {
    /// Creates an empty [`Arena`].
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
            len: 0,
        }
    }

    /// Creates an empty [`Arena`] with pre-allocated slot capacity.
    #[inline]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            free_head: None,
            len: 0,
        }
    }

    /// Returns the number of active nodes currently in the arena.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the arena contains no active nodes.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns `true` if the arena contains an active node matching `id`.
    #[inline]
    #[must_use]
    pub fn contains(&self, id: NodeId) -> bool {
        self.get(id).is_some()
    }

    /// Allocates a new node in the arena using a closure that receives the assigned [`NodeId`].
    pub fn allocate<F>(&mut self, constructor: F) -> NodeId
    where
        F: FnOnce(NodeId) -> InstanceNode,
    {
        if let Some(index) = self.free_head {
            let slot = &mut self.slots[index as usize];
            match slot {
                ArenaSlot::Vacant {
                    generation,
                    next_free,
                } => {
                    let gn = *generation;
                    self.free_head = *next_free;
                    let id = NodeId::new(index, gn);
                    let node = constructor(id);
                    *slot = ArenaSlot::Occupied {
                        generation: gn,
                        node,
                    };
                    self.len += 1;
                    id
                }
                ArenaSlot::Occupied { .. } => unreachable!("corrupt free list in arena"),
            }
        } else {
            let index = self.slots.len() as u32;
            let generation = NonZeroU32::new(1).expect("non-zero initial generation");
            let id = NodeId::new(index, generation);
            let node = constructor(id);
            self.slots.push(ArenaSlot::Occupied { generation, node });
            self.len += 1;
            id
        }
    }

    /// Returns a reference to the [`InstanceNode`] with the matching [`NodeId`].
    #[inline]
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&InstanceNode> {
        let slot = self.slots.get(id.index as usize)?;
        match slot {
            ArenaSlot::Occupied { generation, node } if *generation == id.generation => Some(node),
            _ => None,
        }
    }

    /// Returns a mutable reference to the [`InstanceNode`] with the matching [`NodeId`].
    #[inline]
    #[must_use]
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut InstanceNode> {
        let slot = self.slots.get_mut(id.index as usize)?;
        match slot {
            ArenaSlot::Occupied { generation, node } if *generation == id.generation => Some(node),
            _ => None,
        }
    }

    /// Removes a node from the arena, recycling its slot and incrementing its generation.
    pub fn remove(&mut self, id: NodeId) -> Option<InstanceNode> {
        let slot = self.slots.get_mut(id.index as usize)?;
        match slot {
            ArenaSlot::Occupied { generation, .. } if *generation == id.generation => {
                let next_gen = match generation.get().checked_add(1) {
                    Some(val) => NonZeroU32::new(val).expect("non-zero increment"),
                    None => NonZeroU32::MIN,
                };

                let old_slot = std::mem::replace(
                    slot,
                    ArenaSlot::Vacant {
                        generation: next_gen,
                        next_free: self.free_head,
                    },
                );
                self.free_head = Some(id.index);
                self.len -= 1;

                match old_slot {
                    ArenaSlot::Occupied { node, .. } => Some(node),
                    ArenaSlot::Vacant { .. } => unreachable!(),
                }
            }
            _ => None,
        }
    }

    /// Clears all nodes from the arena, incrementing their generations.
    pub fn clear(&mut self) {
        for (idx, slot) in self.slots.iter_mut().enumerate() {
            if let ArenaSlot::Occupied { generation, .. } = slot {
                let next_gen = match generation.get().checked_add(1) {
                    Some(val) => NonZeroU32::new(val).expect("non-zero increment"),
                    None => NonZeroU32::MIN,
                };
                *slot = ArenaSlot::Vacant {
                    generation: next_gen,
                    next_free: self.free_head,
                };
                self.free_head = Some(idx as u32);
            }
        }
        self.len = 0;
    }

    /// Returns an iterator over all active `(NodeId, &InstanceNode)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &InstanceNode)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(idx, slot)| match slot {
                ArenaSlot::Occupied { generation, node } => {
                    Some((NodeId::new(idx as u32, *generation), node))
                }
                ArenaSlot::Vacant { .. } => None,
            })
    }

    /// Returns an iterator over all active `(NodeId, &mut InstanceNode)` pairs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (NodeId, &mut InstanceNode)> {
        self.slots
            .iter_mut()
            .enumerate()
            .filter_map(|(idx, slot)| match slot {
                ArenaSlot::Occupied { generation, node } => {
                    Some((NodeId::new(idx as u32, *generation), node))
                }
                ArenaSlot::Vacant { .. } => None,
            })
    }

    // -------------------------------------------------------------------------
    // Take-and-Restore Execution Helpers (RFC-0001 §6.2)
    // -------------------------------------------------------------------------

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
        self.with_instance_mut(id, |instance, _arena| {
            let mut ctx = ComponentContext::new(id, invalidated);
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
        self.with_instance_mut(id, |instance, _arena| {
            let mut ctx = ComponentContext::new(id, invalidated);
            match instance.update_erased(msg, &mut ctx) {
                Ok(decision) => Ok(decision),
                Err(_) => Err(ArenaError::DowncastFailed),
            }
        })?
    }

    /// Renders an instance's visual subtree using the Take-and-Restore pattern.
    pub fn render(&mut self, id: NodeId, invalidated: &Cell<bool>) -> Result<UiNode, ArenaError> {
        self.with_instance(id, |instance, _arena| {
            let ctx = RenderContext::new(id, invalidated);
            instance.render_erased(&ctx)
        })
    }

    /// Invokes the `on_mount` lifecycle hook using Take-and-Restore.
    pub fn on_mount(&mut self, id: NodeId, invalidated: &Cell<bool>) -> Result<(), ArenaError> {
        self.with_instance_mut(id, |instance, _arena| {
            let mut ctx = ComponentContext::new(id, invalidated);
            instance.on_mount(&mut ctx);
        })
    }

    /// Invokes the `on_unmount` lifecycle hook using Take-and-Restore.
    pub fn on_unmount(&mut self, id: NodeId, invalidated: &Cell<bool>) -> Result<(), ArenaError> {
        self.with_instance_mut(id, |instance, _arena| {
            let mut ctx = ComponentContext::new(id, invalidated);
            instance.on_unmount(&mut ctx);
        })
    }
}
