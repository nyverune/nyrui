//! Primary generational arena container and slot management.

use super::instance::InstanceNode;
use super::slot::ArenaSlot;
use crate::id::NodeId;

use std::any::TypeId;
use std::num::NonZeroU32;

/// Single-threaded generational arena storing heterogeneous component instances.
///
/// Implements $O(1)$ allocation, $O(1)$ lookup, and $O(1)$ removal with ABA
/// generational protection (RFC-0001 §6.2).
#[derive(Debug, Default)]
pub struct Arena {
    pub(crate) slots: Vec<ArenaSlot>,
    pub(crate) free_head: Option<u32>,
    pub(crate) len: usize,
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

    /// Attaches a typed context value to the node with the matching [`NodeId`].
    pub fn provide_context<T: Clone + 'static>(&self, id: NodeId, value: T) {
        if let Some(node) = self.get(id) {
            node.provide_context(value);
        }
    }

    /// Traverses up the ancestor chain starting from `id` in $O(\text{depth})$
    /// and returns the nearest provided context value of type `T`.
    ///
    /// Checks the starting node `id` first, followed by each parent up to the root.
    /// Returns `None` if no ancestor provides `T`.
    #[must_use]
    pub fn use_context<T: Clone + 'static>(&self, mut curr: NodeId) -> Option<T> {
        let type_id = TypeId::of::<T>();
        loop {
            let node = self.get(curr)?;
            if let Some(boxed) = node.contexts.borrow().get(&type_id) {
                return boxed.downcast_ref::<T>().cloned();
            }
            curr = node.parent?;
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
}
