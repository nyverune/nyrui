//! Generational reactive graph storage, observer tracking, and dirty notification.

use super::computed::{Computed, ComputedId};
use super::error::SignalError;
use super::signal::{Signal, SignalId};
use crate::id::NodeId;

use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::rc::Rc;

/// An active observer registered for automatic dependency tracking in the reactive graph.
///
/// Implements the observer entities specified in **RFC-0001 §7.1**.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum ObserverId {
    /// A mounted component instance node in the arena runtime.
    Node(NodeId),
    /// A derived computation node in the reactive graph.
    Computed(ComputedId),
}

impl From<NodeId> for ObserverId {
    #[inline]
    fn from(id: NodeId) -> Self {
        ObserverId::Node(id)
    }
}

impl From<ComputedId> for ObserverId {
    #[inline]
    fn from(id: ComputedId) -> Self {
        ObserverId::Computed(id)
    }
}

/// Execution state of a derived [`Computed`] reactive node.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ComputedState {
    /// The computed value is up-to-date and valid.
    Clean,
    /// The computation or one of its dependencies has been mutated and requires re-evaluation.
    Dirty,
}

pub(crate) struct SignalEntry {
    pub(crate) generation: NonZeroU32,
    pub(crate) value: RefCell<Box<dyn Any>>,
    pub(crate) type_id: TypeId,
    pub(crate) type_name: &'static str,
    pub(crate) subscribers: RefCell<HashSet<ObserverId>>,
}

pub(crate) enum ReactiveSlot {
    Occupied(SignalEntry),
    Vacant {
        generation: NonZeroU32,
        next_free: Option<u32>,
    },
}

pub(crate) type ComputeFn = Box<dyn FnMut(&ReactiveGraph) -> Box<dyn Any>>;

pub(crate) struct ComputedEntry {
    pub(crate) generation: NonZeroU32,
    pub(crate) value: RefCell<Option<Box<dyn Any>>>,
    pub(crate) compute: RefCell<ComputeFn>,
    pub(crate) state: Cell<ComputedState>,
    pub(crate) type_id: TypeId,
    pub(crate) type_name: &'static str,
    pub(crate) subscribers: RefCell<HashSet<ObserverId>>,
    pub(crate) signal_dependencies: RefCell<HashSet<SignalId>>,
    pub(crate) computed_dependencies: RefCell<HashSet<ComputedId>>,
}

pub(crate) enum ComputedSlot {
    Occupied(Rc<ComputedEntry>),
    Vacant {
        generation: NonZeroU32,
        next_free: Option<u32>,
    },
}

/// Generational arena storage for reactive signals and derived computations with automatic dependency tracking.
///
/// Implements the reactive graph specified in **RFC-0001 §7.1**:
/// - Allocates heterogeneous [`Signal`]s and [`Computed`] expressions in ABA-protected generational arenas.
/// - Automatically tracks reactive dependencies during component render passes and computed evaluations.
/// - Coalesces dirty notifications into a dirty node set without atomic locking.
/// - Resolves diamond dependencies (`A -> B, A -> C, (B, C) -> D`) with at most single evaluation per batch.
/// - Detects transitive dependency cycles on the tracking stack and panics with diagnostic traces.
#[derive(Default)]
pub struct ReactiveGraph {
    pub(crate) slots: RefCell<Vec<ReactiveSlot>>,
    pub(crate) free_head: Cell<Option<u32>>,
    pub(crate) len: Cell<usize>,
    pub(crate) computed_slots: RefCell<Vec<ComputedSlot>>,
    pub(crate) computed_free_head: Cell<Option<u32>>,
    pub(crate) computed_len: Cell<usize>,
    pub(crate) tracking_stack: RefCell<Vec<ObserverId>>,
    pub(crate) node_subscriptions: RefCell<HashMap<NodeId, HashSet<SignalId>>>,
    pub(crate) node_computed_subscriptions: RefCell<HashMap<NodeId, HashSet<ComputedId>>>,
    pub(crate) dirty_nodes: RefCell<HashSet<NodeId>>,
}

impl ReactiveGraph {
    /// Creates a new, empty [`ReactiveGraph`].
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: RefCell::new(Vec::new()),
            free_head: Cell::new(None),
            len: Cell::new(0),
            computed_slots: RefCell::new(Vec::new()),
            computed_free_head: Cell::new(None),
            computed_len: Cell::new(0),
            tracking_stack: RefCell::new(Vec::new()),
            node_subscriptions: RefCell::new(HashMap::new()),
            node_computed_subscriptions: RefCell::new(HashMap::new()),
            dirty_nodes: RefCell::new(HashSet::new()),
        }
    }

    /// Creates an empty [`ReactiveGraph`] with pre-allocated slot capacity.
    #[inline]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: RefCell::new(Vec::with_capacity(capacity)),
            free_head: Cell::new(None),
            len: Cell::new(0),
            computed_slots: RefCell::new(Vec::with_capacity(capacity)),
            computed_free_head: Cell::new(None),
            computed_len: Cell::new(0),
            tracking_stack: RefCell::new(Vec::new()),
            node_subscriptions: RefCell::new(HashMap::new()),
            node_computed_subscriptions: RefCell::new(HashMap::new()),
            dirty_nodes: RefCell::new(HashSet::new()),
        }
    }

    /// Returns the number of active signals currently stored in the graph.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.len.get()
    }

    /// Returns `true` if there are no active signals stored in the graph.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len.get() == 0
    }

    /// Returns the number of active computed computations currently stored in the graph.
    #[inline]
    #[must_use]
    pub fn computed_len(&self) -> usize {
        self.computed_len.get()
    }

    /// Returns `true` if there are no active computed computations stored in the graph.
    #[inline]
    #[must_use]
    pub fn is_computed_empty(&self) -> bool {
        self.computed_len.get() == 0
    }

    /// Returns `true` if the graph contains an active signal matching the given `id`.
    #[must_use]
    pub fn contains(&self, id: SignalId) -> bool {
        let slots = self.slots.borrow();
        match slots.get(id.index as usize) {
            Some(ReactiveSlot::Occupied(entry)) => entry.generation == id.generation,
            _ => false,
        }
    }

    /// Returns `true` if the graph contains an active computed computation matching the given `id`.
    #[must_use]
    pub fn contains_computed(&self, id: ComputedId) -> bool {
        let slots = self.computed_slots.borrow();
        match slots.get(id.index as usize) {
            Some(ComputedSlot::Occupied(entry)) => entry.generation == id.generation,
            _ => false,
        }
    }

    /// Returns the human-readable type name of the signal with the given `id`, if active.
    #[must_use]
    pub fn signal_type_name(&self, id: SignalId) -> Option<&'static str> {
        let slots = self.slots.borrow();
        match slots.get(id.index as usize) {
            Some(ReactiveSlot::Occupied(entry)) if entry.generation == id.generation => {
                Some(entry.type_name)
            }
            _ => None,
        }
    }

    /// Returns the [`TypeId`] of the signal with the given `id`, if active.
    #[must_use]
    pub fn signal_type_id(&self, id: SignalId) -> Option<TypeId> {
        let slots = self.slots.borrow();
        match slots.get(id.index as usize) {
            Some(ReactiveSlot::Occupied(entry)) if entry.generation == id.generation => {
                Some(entry.type_id)
            }
            _ => None,
        }
    }

    /// Creates and allocates a new generational signal with the given `initial_value`.
    pub fn create_signal<T: 'static>(&self, initial_value: T) -> Signal<T> {
        let mut slots = self.slots.borrow_mut();
        let free_head = self.free_head.get();

        let (index, generation) = if let Some(idx) = free_head {
            let slot = &mut slots[idx as usize];
            match slot {
                ReactiveSlot::Vacant {
                    generation,
                    next_free,
                } => {
                    let gen_val = *generation;
                    self.free_head.set(*next_free);
                    let entry = SignalEntry {
                        generation: gen_val,
                        value: RefCell::new(Box::new(initial_value)),
                        type_id: TypeId::of::<T>(),
                        type_name: std::any::type_name::<T>(),
                        subscribers: RefCell::new(HashSet::new()),
                    };
                    *slot = ReactiveSlot::Occupied(entry);
                    self.len.set(self.len.get() + 1);
                    (idx, gen_val)
                }
                ReactiveSlot::Occupied(_) => unreachable!("corrupt free list in reactive graph"),
            }
        } else {
            let idx = slots.len() as u32;
            let gen_val = NonZeroU32::new(1).expect("non-zero initial generation");
            let entry = SignalEntry {
                generation: gen_val,
                value: RefCell::new(Box::new(initial_value)),
                type_id: TypeId::of::<T>(),
                type_name: std::any::type_name::<T>(),
                subscribers: RefCell::new(HashSet::new()),
            };
            slots.push(ReactiveSlot::Occupied(entry));
            self.len.set(self.len.get() + 1);
            (idx, gen_val)
        };

        Signal::from_id(SignalId::new(index, generation))
    }

    /// Disposes of the signal matching `id`, freeing its slot and incrementing its generation.
    ///
    /// Cleans up any registered subscriber relationships. Returns `true` if the signal was found.
    pub fn dispose_signal(&self, signal_id: SignalId) -> bool {
        let mut slots = self.slots.borrow_mut();
        let slot = match slots.get_mut(signal_id.index as usize) {
            Some(s) => s,
            None => return false,
        };

        match slot {
            ReactiveSlot::Occupied(entry) if entry.generation == signal_id.generation => {
                let subs = entry.subscribers.borrow().clone();
                for sub in subs {
                    match sub {
                        ObserverId::Node(node_id) => {
                            if let Some(sigs) =
                                self.node_subscriptions.borrow_mut().get_mut(&node_id)
                            {
                                sigs.remove(&signal_id);
                            }
                        }
                        ObserverId::Computed(comp_id) => {
                            let comp_slots = self.computed_slots.borrow();
                            if let Some(ComputedSlot::Occupied(comp_entry)) =
                                comp_slots.get(comp_id.index as usize)
                            {
                                if comp_entry.generation == comp_id.generation {
                                    comp_entry
                                        .signal_dependencies
                                        .borrow_mut()
                                        .remove(&signal_id);
                                }
                            }
                        }
                    }
                }

                let next_gen = match signal_id.generation.get().checked_add(1) {
                    Some(v) => NonZeroU32::new(v).expect("non-zero increment"),
                    None => NonZeroU32::MIN,
                };

                *slot = ReactiveSlot::Vacant {
                    generation: next_gen,
                    next_free: self.free_head.get(),
                };
                self.free_head.set(Some(signal_id.index));
                self.len.set(self.len.get() - 1);
                true
            }
            _ => false,
        }
    }

    /// Reads and clones the value of `signal`, recording a dependency if within an observer scope.
    ///
    /// # Panics
    ///
    /// Panics if the signal is not found, types mismatch, or if currently mutably borrowed.
    #[inline]
    #[must_use]
    pub fn get_signal<T: Clone + 'static>(&self, signal: Signal<T>) -> T {
        self.try_get_signal(signal).expect("valid signal get")
    }

    /// Attempts to read and clone the value of `signal` without panicking.
    pub fn try_get_signal<T: Clone + 'static>(&self, signal: Signal<T>) -> Result<T, SignalError> {
        self.try_with_signal(signal, T::clone)
    }

    /// Applies a closure to an immutable reference to `signal`'s value, recording a dependency.
    ///
    /// # Panics
    ///
    /// Panics if the signal is not found, types mismatch, or if currently mutably borrowed.
    #[inline]
    pub fn with_signal<T: 'static, R>(&self, signal: Signal<T>, f: impl FnOnce(&T) -> R) -> R {
        self.try_with_signal(signal, f).expect("valid signal with")
    }

    /// Attempts to borrow `signal`'s value immutably without panicking.
    pub fn try_with_signal<T: 'static, R>(
        &self,
        signal: Signal<T>,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, SignalError> {
        let slots = self.slots.borrow();
        let slot = slots
            .get(signal.id.index as usize)
            .ok_or(SignalError::SignalNotFound(signal.id))?;

        match slot {
            ReactiveSlot::Occupied(entry) if entry.generation == signal.id.generation => {
                if entry.type_id != TypeId::of::<T>() {
                    return Err(SignalError::TypeMismatch {
                        expected: entry.type_name,
                        found: std::any::type_name::<T>(),
                    });
                }

                let borrow = entry
                    .value
                    .try_borrow()
                    .map_err(|_| SignalError::AlreadyBorrowed(signal.id))?;
                let val = borrow.downcast_ref::<T>().expect("type verified");

                // Record reactive dependency before evaluating the closure
                self.track(signal.id);

                Ok(f(val))
            }
            _ => Err(SignalError::SignalNotFound(signal.id)),
        }
    }

    /// Sets a new value for `signal`, unconditionally notifying subscribers and marking them dirty.
    ///
    /// # Panics
    ///
    /// Panics if the signal is not found, types mismatch, or if currently borrowed.
    #[inline]
    pub fn set_signal<T: 'static>(&self, signal: Signal<T>, new_val: T) {
        self.try_set_signal(signal, new_val)
            .expect("valid signal set");
    }

    /// Attempts to set a new value for `signal` without panicking.
    pub fn try_set_signal<T: 'static>(
        &self,
        signal: Signal<T>,
        new_val: T,
    ) -> Result<(), SignalError> {
        let slots = self.slots.borrow();
        let slot = slots
            .get(signal.id.index as usize)
            .ok_or(SignalError::SignalNotFound(signal.id))?;

        match slot {
            ReactiveSlot::Occupied(entry) if entry.generation == signal.id.generation => {
                if entry.type_id != TypeId::of::<T>() {
                    return Err(SignalError::TypeMismatch {
                        expected: entry.type_name,
                        found: std::any::type_name::<T>(),
                    });
                }

                {
                    let mut borrow = entry
                        .value
                        .try_borrow_mut()
                        .map_err(|_| SignalError::AlreadyBorrowed(signal.id))?;
                    *borrow = Box::new(new_val);
                }

                self.notify_subscribers(signal.id);
                Ok(())
            }
            _ => Err(SignalError::SignalNotFound(signal.id)),
        }
    }

    /// Updates `signal` with `new_val` if `*current != new_val`.
    ///
    /// If equal, the write is skipped and subscribers are **not** marked dirty, returning `false`.
    /// If different, updates the value and notifies subscribers, returning `true`.
    ///
    /// # Panics
    ///
    /// Panics if the signal is not found, types mismatch, or if currently borrowed.
    #[inline]
    pub fn set_signal_if_changed<T: PartialEq + 'static>(
        &self,
        signal: Signal<T>,
        new_val: T,
    ) -> bool {
        self.try_set_signal_if_changed(signal, new_val)
            .expect("valid signal set_if_changed")
    }

    /// Attempts to compare and set `new_val` without panicking.
    pub fn try_set_signal_if_changed<T: PartialEq + 'static>(
        &self,
        signal: Signal<T>,
        new_val: T,
    ) -> Result<bool, SignalError> {
        let slots = self.slots.borrow();
        let slot = slots
            .get(signal.id.index as usize)
            .ok_or(SignalError::SignalNotFound(signal.id))?;

        match slot {
            ReactiveSlot::Occupied(entry) if entry.generation == signal.id.generation => {
                if entry.type_id != TypeId::of::<T>() {
                    return Err(SignalError::TypeMismatch {
                        expected: entry.type_name,
                        found: std::any::type_name::<T>(),
                    });
                }

                {
                    let borrow = entry
                        .value
                        .try_borrow()
                        .map_err(|_| SignalError::AlreadyBorrowed(signal.id))?;
                    let current = borrow.downcast_ref::<T>().expect("type verified");
                    if current == &new_val {
                        return Ok(false);
                    }
                }

                {
                    let mut borrow = entry
                        .value
                        .try_borrow_mut()
                        .map_err(|_| SignalError::AlreadyBorrowed(signal.id))?;
                    *borrow = Box::new(new_val);
                }

                self.notify_subscribers(signal.id);
                Ok(true)
            }
            _ => Err(SignalError::SignalNotFound(signal.id)),
        }
    }

    /// Mutates `signal`'s value in place, unconditionally notifying subscribers.
    ///
    /// # Panics
    ///
    /// Panics if the signal is not found, types mismatch, or if currently borrowed.
    #[inline]
    pub fn update_signal<T: 'static, R>(
        &self,
        signal: Signal<T>,
        f: impl FnOnce(&mut T) -> R,
    ) -> R {
        self.try_update_signal(signal, f)
            .expect("valid signal update")
    }

    /// Attempts to mutate `signal`'s value in place without panicking.
    pub fn try_update_signal<T: 'static, R>(
        &self,
        signal: Signal<T>,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, SignalError> {
        let slots = self.slots.borrow();
        let slot = slots
            .get(signal.id.index as usize)
            .ok_or(SignalError::SignalNotFound(signal.id))?;

        match slot {
            ReactiveSlot::Occupied(entry) if entry.generation == signal.id.generation => {
                if entry.type_id != TypeId::of::<T>() {
                    return Err(SignalError::TypeMismatch {
                        expected: entry.type_name,
                        found: std::any::type_name::<T>(),
                    });
                }

                let result = {
                    let mut borrow = entry
                        .value
                        .try_borrow_mut()
                        .map_err(|_| SignalError::AlreadyBorrowed(signal.id))?;
                    let val = borrow.downcast_mut::<T>().expect("type verified");
                    f(val)
                };

                self.notify_subscribers(signal.id);
                Ok(result)
            }
            _ => Err(SignalError::SignalNotFound(signal.id)),
        }
    }

    /// Replaces `signal`'s value with `new_val`, returning the previous value and notifying subscribers.
    ///
    /// # Panics
    ///
    /// Panics if the signal is not found, types mismatch, or if currently borrowed.
    #[inline]
    pub fn replace_signal<T: 'static>(&self, signal: Signal<T>, new_val: T) -> T {
        self.try_replace_signal(signal, new_val)
            .expect("valid signal replace")
    }

    /// Attempts to replace `signal`'s value without panicking.
    pub fn try_replace_signal<T: 'static>(
        &self,
        signal: Signal<T>,
        new_val: T,
    ) -> Result<T, SignalError> {
        let slots = self.slots.borrow();
        let slot = slots
            .get(signal.id.index as usize)
            .ok_or(SignalError::SignalNotFound(signal.id))?;

        match slot {
            ReactiveSlot::Occupied(entry) if entry.generation == signal.id.generation => {
                if entry.type_id != TypeId::of::<T>() {
                    return Err(SignalError::TypeMismatch {
                        expected: entry.type_name,
                        found: std::any::type_name::<T>(),
                    });
                }

                let old_val = {
                    let mut borrow = entry
                        .value
                        .try_borrow_mut()
                        .map_err(|_| SignalError::AlreadyBorrowed(signal.id))?;
                    let old_box = std::mem::replace(&mut *borrow, Box::new(new_val));
                    *old_box.downcast::<T>().expect("type verified")
                };

                self.notify_subscribers(signal.id);
                Ok(old_val)
            }
            _ => Err(SignalError::SignalNotFound(signal.id)),
        }
    }

    /// Pushes an active observer onto the dependency tracking stack.
    #[inline]
    pub fn push_observer(&self, observer: ObserverId) {
        self.tracking_stack.borrow_mut().push(observer);
    }

    /// Pops the active observer from the dependency tracking stack.
    #[inline]
    pub fn pop_observer(&self) -> Option<ObserverId> {
        self.tracking_stack.borrow_mut().pop()
    }

    /// Returns the currently active observer, if any.
    #[inline]
    #[must_use]
    pub fn current_observer(&self) -> Option<ObserverId> {
        self.tracking_stack.borrow().last().copied()
    }

    /// Executes `f` within the scope of `observer`, automatically popping it upon completion.
    pub fn with_observer<R>(&self, observer: ObserverId, f: impl FnOnce() -> R) -> R {
        self.push_observer(observer);
        let result = f();
        self.pop_observer();
        result
    }

    /// Records a reactive dependency between `signal_id` and the currently active observer.
    pub fn track(&self, signal_id: SignalId) {
        if let Some(observer) = self.current_observer() {
            let slots = self.slots.borrow();
            if let Some(ReactiveSlot::Occupied(entry)) = slots.get(signal_id.index as usize) {
                if entry.generation == signal_id.generation {
                    entry.subscribers.borrow_mut().insert(observer);
                    match observer {
                        ObserverId::Node(node_id) => {
                            self.node_subscriptions
                                .borrow_mut()
                                .entry(node_id)
                                .or_default()
                                .insert(signal_id);
                        }
                        ObserverId::Computed(comp_id) => {
                            let comp_slots = self.computed_slots.borrow();
                            if let Some(ComputedSlot::Occupied(comp_entry)) =
                                comp_slots.get(comp_id.index as usize)
                            {
                                if comp_entry.generation == comp_id.generation {
                                    comp_entry
                                        .signal_dependencies
                                        .borrow_mut()
                                        .insert(signal_id);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Returns a set of all active observers subscribed to `signal_id`, or `None` if not found.
    #[must_use]
    pub fn subscribers(&self, signal_id: SignalId) -> Option<HashSet<ObserverId>> {
        let slots = self.slots.borrow();
        match slots.get(signal_id.index as usize) {
            Some(ReactiveSlot::Occupied(entry)) if entry.generation == signal_id.generation => {
                Some(entry.subscribers.borrow().clone())
            }
            _ => None,
        }
    }

    /// Returns the number of subscribers currently observing `signal_id`.
    #[must_use]
    pub fn subscriber_count(&self, signal_id: SignalId) -> usize {
        self.subscribers(signal_id).map_or(0, |s| s.len())
    }

    /// Clears all signal and computed subscriptions registered by `node_id`.
    ///
    /// Called before a component re-renders or when it is unmounted to prevent stale wakeups.
    pub fn clear_node_subscriptions(&self, node_id: NodeId) {
        if let Some(signal_ids) = self.node_subscriptions.borrow_mut().remove(&node_id) {
            let slots = self.slots.borrow();
            for sig_id in signal_ids {
                if let Some(ReactiveSlot::Occupied(entry)) = slots.get(sig_id.index as usize) {
                    if entry.generation == sig_id.generation {
                        entry
                            .subscribers
                            .borrow_mut()
                            .remove(&ObserverId::Node(node_id));
                    }
                }
            }
        }

        if let Some(computed_ids) = self
            .node_computed_subscriptions
            .borrow_mut()
            .remove(&node_id)
        {
            let slots = self.computed_slots.borrow();
            for comp_id in computed_ids {
                if let Some(ComputedSlot::Occupied(entry)) = slots.get(comp_id.index as usize) {
                    if entry.generation == comp_id.generation {
                        entry
                            .subscribers
                            .borrow_mut()
                            .remove(&ObserverId::Node(node_id));
                    }
                }
            }
        }
    }

    /// Returns all signal IDs currently observed by `node_id`.
    #[must_use]
    pub fn node_subscriptions(&self, node_id: NodeId) -> HashSet<SignalId> {
        self.node_subscriptions
            .borrow()
            .get(&node_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Returns all computed IDs currently observed by `node_id`.
    #[must_use]
    pub fn node_computed_subscriptions(&self, node_id: NodeId) -> HashSet<ComputedId> {
        self.node_computed_subscriptions
            .borrow()
            .get(&node_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Marks all subscribers of `signal_id` as dirty in the scheduler queue and marks dependent computeds dirty.
    pub fn notify_subscribers(&self, signal_id: SignalId) {
        let slots = self.slots.borrow();
        if let Some(ReactiveSlot::Occupied(entry)) = slots.get(signal_id.index as usize) {
            if entry.generation == signal_id.generation {
                let subs = entry.subscribers.borrow().clone();
                for sub in subs {
                    match sub {
                        ObserverId::Node(node_id) => {
                            self.dirty_nodes.borrow_mut().insert(node_id);
                        }
                        ObserverId::Computed(comp_id) => {
                            self.mark_computed_dirty(comp_id);
                        }
                    }
                }
            }
        }
    }

    /// Creates and allocates a new derived computation with lazy evaluation.
    pub fn create_computed<T: 'static>(&self, compute: impl Fn() -> T + 'static) -> Computed<T> {
        self.create_computed_with_context(move |_| compute())
    }

    /// Creates and allocates a new derived computation whose closure receives `&ReactiveGraph`.
    pub fn create_computed_with_context<T: 'static>(
        &self,
        compute: impl Fn(&ReactiveGraph) -> T + 'static,
    ) -> Computed<T> {
        let compute_fn = compute;
        let erased_compute =
            Box::new(move |graph: &ReactiveGraph| -> Box<dyn Any> { Box::new(compute_fn(graph)) });

        let mut slots = self.computed_slots.borrow_mut();
        let free_head = self.computed_free_head.get();

        let (index, generation) = if let Some(idx) = free_head {
            let slot = &mut slots[idx as usize];
            match slot {
                ComputedSlot::Vacant {
                    generation,
                    next_free,
                } => {
                    let gen_val = *generation;
                    self.computed_free_head.set(*next_free);
                    let entry = ComputedEntry {
                        generation: gen_val,
                        value: RefCell::new(None),
                        compute: RefCell::new(erased_compute),
                        state: Cell::new(ComputedState::Dirty),
                        type_id: TypeId::of::<T>(),
                        type_name: std::any::type_name::<T>(),
                        subscribers: RefCell::new(HashSet::new()),
                        signal_dependencies: RefCell::new(HashSet::new()),
                        computed_dependencies: RefCell::new(HashSet::new()),
                    };
                    *slot = ComputedSlot::Occupied(Rc::new(entry));
                    self.computed_len.set(self.computed_len.get() + 1);
                    (idx, gen_val)
                }
                ComputedSlot::Occupied(_) => unreachable!("corrupt free list in computed slots"),
            }
        } else {
            let idx = slots.len() as u32;
            let gen_val = NonZeroU32::new(1).expect("non-zero initial generation");
            let entry = ComputedEntry {
                generation: gen_val,
                value: RefCell::new(None),
                compute: RefCell::new(erased_compute),
                state: Cell::new(ComputedState::Dirty),
                type_id: TypeId::of::<T>(),
                type_name: std::any::type_name::<T>(),
                subscribers: RefCell::new(HashSet::new()),
                signal_dependencies: RefCell::new(HashSet::new()),
                computed_dependencies: RefCell::new(HashSet::new()),
            };
            slots.push(ComputedSlot::Occupied(Rc::new(entry)));
            self.computed_len.set(self.computed_len.get() + 1);
            (idx, gen_val)
        };

        Computed::from_id(ComputedId::new(index, generation))
    }

    /// Reads and clones the current cached value of `computed`, evaluating lazily if dirty.
    ///
    /// # Panics
    ///
    /// Panics if a dependency cycle is detected or lookup fails.
    #[inline]
    #[must_use]
    pub fn get_computed<T: Clone + 'static>(&self, computed: Computed<T>) -> T {
        self.try_get_computed(computed).expect("valid computed get")
    }

    /// Attempts to read and clone the current value of `computed` without panicking on lookup errors.
    ///
    /// # Panics
    ///
    /// Panics if a dependency cycle is detected.
    pub fn try_get_computed<T: Clone + 'static>(
        &self,
        computed: Computed<T>,
    ) -> Result<T, SignalError> {
        self.try_with_computed(computed, T::clone)
    }

    /// Applies a closure to the borrowed reference of `computed`'s cached value, evaluating lazily if dirty.
    ///
    /// # Panics
    ///
    /// Panics if a dependency cycle is detected or lookup fails.
    #[inline]
    pub fn with_computed<T: 'static, R>(
        &self,
        computed: Computed<T>,
        f: impl FnOnce(&T) -> R,
    ) -> R {
        self.try_with_computed(computed, f)
            .expect("valid computed with")
    }

    /// Attempts to borrow the computed value without panicking on lookup errors.
    ///
    /// Implements the cycle detection and lazy push-pull evaluation contracts of **RFC-0001 §7.1**.
    ///
    /// # Panics
    ///
    /// Panics with a diagnostic trace if a cycle is detected on the active tracking stack.
    pub fn try_with_computed<T: 'static, R>(
        &self,
        computed: Computed<T>,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, SignalError> {
        // 1. Cycle detection: if Computed(id) is already on the tracking stack, panic with trace
        if self
            .tracking_stack
            .borrow()
            .contains(&ObserverId::Computed(computed.id))
        {
            let stack = self.tracking_stack.borrow();
            let mut trace = String::new();
            for obs in stack.iter() {
                trace.push_str(&format!("  -> {obs:?}\n"));
            }
            trace.push_str(&format!(
                "  -> ObserverId::Computed({}) [CYCLE]",
                computed.id
            ));
            panic!(
                "Dependency cycle detected in reactive graph: Computed {} is already being evaluated on the active tracking stack.\nActive tracking stack:\n{}",
                computed.id, trace
            );
        }

        // 2. Lookup and clone Rc<ComputedEntry>
        let entry = {
            let slots = self.computed_slots.borrow();
            match slots.get(computed.id.index as usize) {
                Some(ComputedSlot::Occupied(e)) if e.generation == computed.id.generation => {
                    if e.type_id != TypeId::of::<T>() {
                        return Err(SignalError::TypeMismatch {
                            expected: e.type_name,
                            found: std::any::type_name::<T>(),
                        });
                    }
                    e.clone()
                }
                _ => return Err(SignalError::ComputedNotFound(computed.id)),
            }
        };

        // 3. Register dependency with current observer
        if let Some(observer) = self.current_observer() {
            entry.subscribers.borrow_mut().insert(observer);
            match observer {
                ObserverId::Node(node_id) => {
                    self.node_computed_subscriptions
                        .borrow_mut()
                        .entry(node_id)
                        .or_default()
                        .insert(computed.id);
                }
                ObserverId::Computed(caller_comp_id) => {
                    let caller_entry = self.get_computed_entry(caller_comp_id);
                    if let Some(caller) = caller_entry {
                        caller
                            .computed_dependencies
                            .borrow_mut()
                            .insert(computed.id);
                    }
                }
            }
        }

        // 4. Ensure computed is Clean (re-evaluate if Dirty)
        if entry.state.get() == ComputedState::Dirty {
            self.clear_computed_dependencies(computed.id);

            struct ObserverGuard<'a>(&'a ReactiveGraph);
            impl Drop for ObserverGuard<'_> {
                fn drop(&mut self) {
                    self.0.pop_observer();
                }
            }

            let _guard = ObserverGuard(self);
            self.push_observer(ObserverId::Computed(computed.id));

            let new_box = {
                let mut compute_borrow = entry
                    .compute
                    .try_borrow_mut()
                    .map_err(|_| SignalError::ComputedAlreadyBorrowed(computed.id))?;
                compute_borrow(self)
            };

            drop(_guard);

            *entry.value.borrow_mut() = Some(new_box);
            entry.state.set(ComputedState::Clean);
        }

        // 5. Read cached value
        let val_borrow = entry.value.borrow();
        let val_box = val_borrow
            .as_ref()
            .expect("computed value must be present after clean evaluation");
        let val = val_box
            .downcast_ref::<T>()
            .expect("type verified during entry lookup");
        Ok(f(val))
    }

    /// Reads and clones the current computed value without registering an active dependency.
    #[must_use]
    pub fn get_computed_untracked<T: Clone + 'static>(&self, computed: Computed<T>) -> T {
        self.with_computed_untracked(computed, T::clone)
    }

    /// Applies a closure to the borrowed reference without registering an active dependency.
    pub fn with_computed_untracked<T: 'static, R>(
        &self,
        computed: Computed<T>,
        f: impl FnOnce(&T) -> R,
    ) -> R {
        let saved_stack = std::mem::take(&mut *self.tracking_stack.borrow_mut());
        let result = self.with_computed(computed, f);
        *self.tracking_stack.borrow_mut() = saved_stack;
        result
    }

    /// Eagerly marks a computed node and all its downstream observers dirty.
    pub fn mark_computed_dirty(&self, id: ComputedId) {
        let entry = {
            let slots = self.computed_slots.borrow();
            match slots.get(id.index as usize) {
                Some(ComputedSlot::Occupied(e)) if e.generation == id.generation => e.clone(),
                _ => return,
            }
        };

        if entry.state.get() == ComputedState::Dirty {
            return; // Already dirty, prune propagation
        }
        entry.state.set(ComputedState::Dirty);

        let subs = entry.subscribers.borrow().clone();
        for sub in subs {
            match sub {
                ObserverId::Node(node_id) => {
                    self.dirty_nodes.borrow_mut().insert(node_id);
                }
                ObserverId::Computed(child_id) => {
                    self.mark_computed_dirty(child_id);
                }
            }
        }
    }

    /// Clears all upstream signal and computed dependencies registered by `comp_id`.
    pub fn clear_computed_dependencies(&self, comp_id: ComputedId) {
        let (signal_deps, computed_deps) = {
            let slots = self.computed_slots.borrow();
            match slots.get(comp_id.index as usize) {
                Some(ComputedSlot::Occupied(entry)) if entry.generation == comp_id.generation => {
                    let sigs = std::mem::take(&mut *entry.signal_dependencies.borrow_mut());
                    let comps = std::mem::take(&mut *entry.computed_dependencies.borrow_mut());
                    (sigs, comps)
                }
                _ => return,
            }
        };

        let sig_slots = self.slots.borrow();
        for sig_id in signal_deps {
            if let Some(ReactiveSlot::Occupied(entry)) = sig_slots.get(sig_id.index as usize) {
                if entry.generation == sig_id.generation {
                    entry
                        .subscribers
                        .borrow_mut()
                        .remove(&ObserverId::Computed(comp_id));
                }
            }
        }

        let comp_slots = self.computed_slots.borrow();
        for other_id in computed_deps {
            if let Some(ComputedSlot::Occupied(entry)) = comp_slots.get(other_id.index as usize) {
                if entry.generation == other_id.generation {
                    entry
                        .subscribers
                        .borrow_mut()
                        .remove(&ObserverId::Computed(comp_id));
                }
            }
        }
    }

    /// Disposes of the computed computation matching `id`, freeing its slot and incrementing its generation.
    pub fn dispose_computed(&self, id: ComputedId) -> bool {
        self.clear_computed_dependencies(id);

        let subs = {
            let slots = self.computed_slots.borrow();
            match slots.get(id.index as usize) {
                Some(ComputedSlot::Occupied(entry)) if entry.generation == id.generation => {
                    entry.subscribers.borrow().clone()
                }
                _ => return false,
            }
        };

        for sub in subs {
            match sub {
                ObserverId::Node(node_id) => {
                    if let Some(comps) = self
                        .node_computed_subscriptions
                        .borrow_mut()
                        .get_mut(&node_id)
                    {
                        comps.remove(&id);
                    }
                }
                ObserverId::Computed(other_id) => {
                    let slots = self.computed_slots.borrow();
                    if let Some(ComputedSlot::Occupied(other_entry)) =
                        slots.get(other_id.index as usize)
                    {
                        if other_entry.generation == other_id.generation {
                            other_entry.computed_dependencies.borrow_mut().remove(&id);
                        }
                    }
                }
            }
        }

        let mut slots = self.computed_slots.borrow_mut();
        let slot = match slots.get_mut(id.index as usize) {
            Some(s) => s,
            None => return false,
        };

        match slot {
            ComputedSlot::Occupied(entry) if entry.generation == id.generation => {
                let next_gen = match id.generation.get().checked_add(1) {
                    Some(v) => NonZeroU32::new(v).expect("non-zero increment"),
                    None => NonZeroU32::MIN,
                };
                *slot = ComputedSlot::Vacant {
                    generation: next_gen,
                    next_free: self.computed_free_head.get(),
                };
                self.computed_free_head.set(Some(id.index));
                self.computed_len.set(self.computed_len.get() - 1);
                true
            }
            _ => false,
        }
    }

    /// Returns the current evaluation state of `id`, or `None` if not found.
    #[must_use]
    pub fn computed_state(&self, id: ComputedId) -> Option<ComputedState> {
        let slots = self.computed_slots.borrow();
        match slots.get(id.index as usize) {
            Some(ComputedSlot::Occupied(entry)) if entry.generation == id.generation => {
                Some(entry.state.get())
            }
            _ => None,
        }
    }

    /// Returns a set of all active observers subscribed to `id`, or `None` if not found.
    #[must_use]
    pub fn computed_subscribers(&self, id: ComputedId) -> Option<HashSet<ObserverId>> {
        let slots = self.computed_slots.borrow();
        match slots.get(id.index as usize) {
            Some(ComputedSlot::Occupied(entry)) if entry.generation == id.generation => {
                Some(entry.subscribers.borrow().clone())
            }
            _ => None,
        }
    }

    pub(crate) fn get_computed_entry(&self, id: ComputedId) -> Option<Rc<ComputedEntry>> {
        let slots = self.computed_slots.borrow();
        match slots.get(id.index as usize) {
            Some(ComputedSlot::Occupied(e)) if e.generation == id.generation => Some(e.clone()),
            _ => None,
        }
    }

    /// Returns a cloned set of all `NodeId`s currently marked dirty.
    #[must_use]
    pub fn dirty_nodes(&self) -> HashSet<NodeId> {
        self.dirty_nodes.borrow().clone()
    }

    /// Drains and returns all currently dirty `NodeId`s, clearing the dirty set.
    #[must_use]
    pub fn take_dirty_nodes(&self) -> Vec<NodeId> {
        self.dirty_nodes.borrow_mut().drain().collect()
    }

    /// Returns `true` if `node_id` is currently marked dirty.
    #[must_use]
    pub fn is_node_dirty(&self, node_id: NodeId) -> bool {
        self.dirty_nodes.borrow().contains(&node_id)
    }

    /// Manually marks `node_id` as dirty.
    pub fn mark_node_dirty(&self, node_id: NodeId) {
        self.dirty_nodes.borrow_mut().insert(node_id);
    }

    /// Clears all dirty node marks.
    pub fn clear_dirty_nodes(&self) {
        self.dirty_nodes.borrow_mut().clear();
    }

    /// Returns `true` if there are any nodes currently marked dirty.
    #[must_use]
    pub fn has_dirty_nodes(&self) -> bool {
        !self.dirty_nodes.borrow().is_empty()
    }

    /// Clears all signals, computations, and state from the graph, incrementing generations for active slots.
    pub fn clear(&self) {
        let mut slots = self.slots.borrow_mut();
        for (idx, slot) in slots.iter_mut().enumerate() {
            if let ReactiveSlot::Occupied(entry) = slot {
                let next_gen = match entry.generation.get().checked_add(1) {
                    Some(v) => NonZeroU32::new(v).expect("non-zero increment"),
                    None => NonZeroU32::MIN,
                };
                *slot = ReactiveSlot::Vacant {
                    generation: next_gen,
                    next_free: self.free_head.get(),
                };
                self.free_head.set(Some(idx as u32));
            }
        }
        self.len.set(0);

        let mut comp_slots = self.computed_slots.borrow_mut();
        for (idx, slot) in comp_slots.iter_mut().enumerate() {
            if let ComputedSlot::Occupied(entry) = slot {
                let next_gen = match entry.generation.get().checked_add(1) {
                    Some(v) => NonZeroU32::new(v).expect("non-zero increment"),
                    None => NonZeroU32::MIN,
                };
                *slot = ComputedSlot::Vacant {
                    generation: next_gen,
                    next_free: self.computed_free_head.get(),
                };
                self.computed_free_head.set(Some(idx as u32));
            }
        }
        self.computed_len.set(0);

        self.tracking_stack.borrow_mut().clear();
        self.node_subscriptions.borrow_mut().clear();
        self.node_computed_subscriptions.borrow_mut().clear();
        self.dirty_nodes.borrow_mut().clear();
    }
}

impl std::fmt::Debug for ReactiveGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReactiveGraph")
            .field("len", &self.len())
            .field("computed_len", &self.computed_len())
            .field("dirty_nodes_len", &self.dirty_nodes.borrow().len())
            .field(
                "active_observer",
                &self.tracking_stack.borrow().last().copied(),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_node_id(index: u32) -> NodeId {
        NodeId::new(index, NonZeroU32::new(1).expect("non-zero"))
    }

    #[test]
    fn graph_initial_state() {
        let graph = ReactiveGraph::new();
        assert_eq!(graph.len(), 0);
        assert!(graph.is_empty());
        assert!(!graph.has_dirty_nodes());
    }

    #[test]
    fn graph_signal_allocation_and_disposal() {
        let graph = ReactiveGraph::new();
        let sig1 = graph.create_signal(10);
        let sig2 = graph.create_signal("hello");

        assert_eq!(graph.len(), 2);
        assert!(graph.contains(sig1.id()));
        assert!(graph.contains(sig2.id()));

        assert!(graph.dispose_signal(sig1.id()));
        assert_eq!(graph.len(), 1);
        assert!(!graph.contains(sig1.id()));
        assert!(graph.contains(sig2.id()));

        // Slot reuse with generational protection
        let sig3 = graph.create_signal(20);
        assert_eq!(graph.len(), 2);
        assert_eq!(sig3.id().index, sig1.id().index);
        assert_ne!(sig3.id().generation, sig1.id().generation);
        assert!(!graph.contains(sig1.id()));
        assert!(graph.contains(sig3.id()));
    }

    #[test]
    fn graph_dependency_tracking_and_notification() {
        let graph = ReactiveGraph::new();
        let sig = graph.create_signal(42);

        let node1 = test_node_id(1);
        let node2 = test_node_id(2);

        // Observer 1 tracks signal
        graph.with_observer(ObserverId::Node(node1), || {
            let val = sig.get(&graph);
            assert_eq!(val, 42);
        });

        // Observer 2 tracks signal
        graph.with_observer(ObserverId::Node(node2), || {
            sig.with(&graph, |val| {
                assert_eq!(*val, 42);
            });
        });

        assert_eq!(graph.subscriber_count(sig.id()), 2);
        assert_eq!(graph.node_subscriptions(node1).len(), 1);
        assert_eq!(graph.node_subscriptions(node2).len(), 1);
        assert!(!graph.has_dirty_nodes());

        // Mutating signal dirties both observers
        sig.set(&graph, 99);
        assert!(graph.has_dirty_nodes());
        let dirty = graph.dirty_nodes();
        assert!(dirty.contains(&node1));
        assert!(dirty.contains(&node2));
        assert_eq!(graph.take_dirty_nodes().len(), 2);
        assert!(!graph.has_dirty_nodes());

        // Cleanup subscriptions for node1
        graph.clear_node_subscriptions(node1);
        assert_eq!(graph.subscriber_count(sig.id()), 1);

        // Mutating signal now only dirties node2
        sig.set(&graph, 100);
        let dirty = graph.take_dirty_nodes();
        assert_eq!(dirty, vec![node2]);
    }
}
