//! Mounted instance metadata and type-erased component container.

use crate::component::Component;
use crate::id::{NodeId, NodeKey};
use crate::reactivity::SignalId;
use crate::runtime::erased::{ComponentInstance, ErasedComponent};
use crate::runtime::flags::NodeFlags;

use smallvec::SmallVec;
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;

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
    /// Active signal subscriptions registered during rendering passes.
    pub subscriptions: SmallVec<[SignalId; 4]>,
    /// Type-erased component instance.
    ///
    /// Wrapped in an [`Option`] to support the Take-and-Restore pattern (RFC-0001 §6.2)
    /// where the instance is temporarily removed during lifecycle/update calls.
    pub instance: Option<Box<dyn ErasedComponent>>,
    /// Scoped context values attached to this instance node.
    pub contexts: RefCell<HashMap<TypeId, Box<dyn Any>>>,
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
            subscriptions: SmallVec::new(),
            instance: Some(Box::new(ComponentInstance::new(component, props))),
            contexts: RefCell::new(HashMap::new()),
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
            subscriptions: SmallVec::new(),
            instance: Some(instance),
            contexts: RefCell::new(HashMap::new()),
        }
    }

    /// Creates an empty [`InstanceNode`] placeholder before component instance factory evaluation.
    #[inline]
    #[must_use]
    pub fn placeholder(
        id: NodeId,
        parent: Option<NodeId>,
        depth: u16,
        key: NodeKey,
        type_id: TypeId,
    ) -> Self {
        Self {
            id,
            parent,
            children: Vec::new(),
            type_id,
            key,
            depth,
            flags: NodeFlags::DIRTY | NodeFlags::MOUNTED,
            subscriptions: SmallVec::new(),
            instance: None,
            contexts: RefCell::new(HashMap::new()),
        }
    }

    /// Attaches or replaces a typed context value on this instance.
    pub fn provide_context<T: Clone + 'static>(&self, value: T) {
        self.contexts
            .borrow_mut()
            .insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Retrieves a cloned context value if provided directly on this instance.
    #[must_use]
    pub fn get_context<T: Clone + 'static>(&self) -> Option<T> {
        self.contexts
            .borrow()
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_ref::<T>().cloned())
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
            .field("subscriptions", &self.subscriptions)
            .field("has_instance", &self.instance.is_some())
            .field("contexts_len", &self.contexts.borrow().len())
            .finish()
    }
}
