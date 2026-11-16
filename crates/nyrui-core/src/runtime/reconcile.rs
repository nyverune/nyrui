//! Child list reconciliation matching algorithm with fast-path prefix/suffix diffing.
//!
//! Implements the structural reconciliation engine specified in **RFC-0001 §6.3**:
//! - Fast-path common prefix and common suffix matching.
//! - In-place property updates and [`crate::props::Props::props_eq`] bailout.
//! - Keyed and positional middle-slice matching using [`SmallVec`] (for $\le 8$ items)
//!   or hash lookup (for $> 8$ items).
//! - Duplicate explicit key detection among siblings.

use crate::component::context::ComponentContext;
use crate::id::{Key, NodeId, NodeKey};
use crate::node::UiNode;
use crate::node::component::ComponentDescriptor;
use crate::runtime::arena::{Arena, InstanceNode, NodeFlags};

use smallvec::SmallVec;
use std::any::{Any, TypeId};
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};

/// Result of reconciling a child list against newly rendered [`UiNode`] descriptions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReconciliationResult {
    /// Ordered list of active child [`NodeId`]s following reconciliation.
    pub children: Vec<NodeId>,
    /// Old child [`NodeId`]s that were unmatched and must be unmounted.
    pub unmounted: Vec<NodeId>,
    /// Newly allocated [`NodeId`]s that must have `on_mount` scheduled.
    pub newly_mounted: Vec<NodeId>,
}

/// Recursively flattens a sequence of [`UiNode`]s, unfolding fragments and converting
/// text nodes into component descriptors while discarding empty nodes.
pub fn flatten_ui_nodes(nodes: Vec<UiNode>) -> Vec<ComponentDescriptor> {
    let mut descriptors = Vec::with_capacity(nodes.len());
    flatten_into(&mut descriptors, nodes);
    descriptors
}

fn flatten_into(out: &mut Vec<ComponentDescriptor>, nodes: Vec<UiNode>) {
    for node in nodes {
        match node {
            UiNode::Component(desc) => out.push(desc),
            UiNode::Text(text) => out.push(text.into_descriptor()),
            UiNode::Fragment(children) => flatten_into(out, children),
            UiNode::Empty => {}
        }
    }
}

/// Detects duplicate explicit keys among sibling components.
///
/// In debug/test builds, duplicate explicit keys trigger a panic via `debug_assert!`.
/// In release builds, a structured warning is emitted.
pub fn check_duplicate_explicit_keys(descriptors: &[ComponentDescriptor]) {
    let explicit_keys: SmallVec<[&Key; 8]> = descriptors
        .iter()
        .filter_map(|d| match &d.key {
            NodeKey::Explicit(key) => Some(key),
            NodeKey::Positional { .. } => None,
        })
        .collect();

    if explicit_keys.len() <= 8 {
        for i in 0..explicit_keys.len() {
            for j in (i + 1)..explicit_keys.len() {
                if explicit_keys[i] == explicit_keys[j] {
                    emit_duplicate_key_warning(explicit_keys[i]);
                }
            }
        }
    } else {
        let mut seen = std::collections::HashSet::with_capacity(explicit_keys.len());
        for key in explicit_keys {
            if !seen.insert(key) {
                emit_duplicate_key_warning(key);
            }
        }
    }
}

#[inline]
fn emit_duplicate_key_warning(key: &Key) {
    #[cfg(debug_assertions)]
    {
        panic!(
            "Duplicate explicit key `{:?}` detected among sibling components",
            key
        );
    }
    #[cfg(not(debug_assertions))]
    {
        eprintln!(
            "[nyrui::warn] Duplicate explicit key `{:?}` detected among sibling components",
            key
        );
    }
}

/// Reconciles an existing child [`NodeId`] slice against newly rendered [`UiNode`] descriptions.
///
/// Executes the three-phase reconciliation algorithm from **RFC-0001 §6.3**:
/// 1. Common prefix and common suffix fast-path matching.
/// 2. Middle slice keyed matching with $O(1)$ SmallVec linear scan or hash map lookup.
/// 3. Collection of unmounted leftovers and newly mounted instances.
pub fn reconcile_children(
    arena: &mut Arena,
    parent_id: Option<NodeId>,
    depth: u16,
    old_children: &[NodeId],
    new_ui_nodes: Vec<UiNode>,
    dirty_cell: &Cell<bool>,
) -> ReconciliationResult {
    let new_descriptors = flatten_ui_nodes(new_ui_nodes);
    check_duplicate_explicit_keys(&new_descriptors);

    let old_len = old_children.len();
    let new_len = new_descriptors.len();

    let mut new_children: Vec<Option<ComponentDescriptor>> =
        new_descriptors.into_iter().map(Some).collect();

    let mut old_start = 0;
    let mut old_end = old_len;
    let mut new_start = 0;
    let mut new_end = new_len;

    let mut prefix_matched = Vec::new();
    let mut suffix_matched = Vec::new();
    let mut newly_mounted = Vec::new();
    let mut unmounted = Vec::new();

    // -------------------------------------------------------------------------
    // Phase 1: Common Prefix Fast-Path
    // -------------------------------------------------------------------------
    while old_start < old_end && new_start < new_end {
        let old_id = old_children[old_start];
        let new_desc = new_children[new_start]
            .as_ref()
            .expect("prefix new child descriptor must be present");

        let matches = if let Some(old_node) = arena.get(old_id) {
            old_node.type_id == new_desc.type_id && old_node.key == new_desc.key
        } else {
            false
        };

        if matches {
            let desc = new_children[new_start]
                .take()
                .expect("prefix new child descriptor must be present");
            update_matched_child(arena, old_id, desc.props, dirty_cell);
            prefix_matched.push(old_id);
            old_start += 1;
            new_start += 1;
        } else {
            break;
        }
    }

    // -------------------------------------------------------------------------
    // Phase 2: Common Suffix Fast-Path
    // -------------------------------------------------------------------------
    while old_end > old_start && new_end > new_start {
        let old_id = old_children[old_end - 1];
        let new_desc = new_children[new_end - 1]
            .as_ref()
            .expect("suffix new child descriptor must be present");

        let matches = if let Some(old_node) = arena.get(old_id) {
            old_node.type_id == new_desc.type_id && old_node.key == new_desc.key
        } else {
            false
        };

        if matches {
            let desc = new_children[new_end - 1]
                .take()
                .expect("suffix new child descriptor must be present");
            update_matched_child(arena, old_id, desc.props, dirty_cell);
            suffix_matched.push(old_id);
            old_end -= 1;
            new_end -= 1;
        } else {
            break;
        }
    }

    // -------------------------------------------------------------------------
    // Phase 3: Remaining Middle Slice Matching
    // -------------------------------------------------------------------------
    let mut middle_reconciled = Vec::new();

    if old_start < old_end && new_start < new_end {
        let remaining_old = &old_children[old_start..old_end];
        let old_slice_len = remaining_old.len();

        if old_slice_len <= 8 {
            // SmallVec linear scan (zero heap allocations)
            let mut old_indexed: SmallVec<[(TypeId, NodeKey, NodeId, bool); 8]> =
                SmallVec::with_capacity(old_slice_len);

            for &old_id in remaining_old {
                if let Some(node) = arena.get(old_id) {
                    old_indexed.push((node.type_id, node.key.clone(), old_id, false));
                }
            }

            for slot in &mut new_children[new_start..new_end] {
                let new_desc = slot
                    .take()
                    .expect("middle new child descriptor must be present");
                let mut matched_id = None;
                for item in &mut old_indexed {
                    if !item.3 && item.0 == new_desc.type_id && item.1 == new_desc.key {
                        item.3 = true;
                        matched_id = Some(item.2);
                        break;
                    }
                }

                if let Some(old_id) = matched_id {
                    update_matched_child(arena, old_id, new_desc.props, dirty_cell);
                    middle_reconciled.push(old_id);
                } else {
                    let new_id = mount_node(arena, parent_id, depth, new_desc, dirty_cell);
                    newly_mounted.push(new_id);
                    middle_reconciled.push(new_id);
                }
            }

            for item in &old_indexed {
                if !item.3 {
                    unmounted.push(item.2);
                }
            }
        } else {
            // Hash table lookup for larger lists (> 8 items)
            let mut old_map: HashMap<(TypeId, NodeKey), VecDeque<NodeId>> =
                HashMap::with_capacity(old_slice_len);

            for &old_id in remaining_old {
                if let Some(node) = arena.get(old_id) {
                    old_map
                        .entry((node.type_id, node.key.clone()))
                        .or_default()
                        .push_back(old_id);
                }
            }

            for slot in &mut new_children[new_start..new_end] {
                let new_desc = slot
                    .take()
                    .expect("middle new child descriptor must be present");
                let key = (new_desc.type_id, new_desc.key.clone());
                let matched_id = old_map.get_mut(&key).and_then(|queue| queue.pop_front());

                if let Some(old_id) = matched_id {
                    update_matched_child(arena, old_id, new_desc.props, dirty_cell);
                    middle_reconciled.push(old_id);
                } else {
                    let new_id = mount_node(arena, parent_id, depth, new_desc, dirty_cell);
                    newly_mounted.push(new_id);
                    middle_reconciled.push(new_id);
                }
            }

            for queue in old_map.values() {
                unmounted.extend(queue.iter().copied());
            }
        }
    } else if old_start < old_end {
        // Remaining old children without new counterparts are unmounted
        unmounted.extend_from_slice(&old_children[old_start..old_end]);
    } else if new_start < new_end {
        // Remaining new children without old counterparts are freshly allocated
        for slot in &mut new_children[new_start..new_end] {
            let new_desc = slot
                .take()
                .expect("remaining new child descriptor must be present");
            let new_id = mount_node(arena, parent_id, depth, new_desc, dirty_cell);
            newly_mounted.push(new_id);
            middle_reconciled.push(new_id);
        }
    }

    // Unmount all unmatched old children in post-order (children before parent)
    for &unmounted_id in &unmounted {
        unmount_subtree(arena, unmounted_id, dirty_cell);
    }

    // Assemble final reconciled children list
    let mut final_children =
        Vec::with_capacity(prefix_matched.len() + middle_reconciled.len() + suffix_matched.len());
    final_children.extend(prefix_matched);
    final_children.extend(middle_reconciled);
    suffix_matched.reverse();
    final_children.extend(suffix_matched);

    // Update parent's children list if parent is present in arena
    if let Some(pid) = parent_id {
        if let Some(parent) = arena.get_mut(pid) {
            parent.children = final_children.clone();
        }
    }

    ReconciliationResult {
        children: final_children,
        unmounted,
        newly_mounted,
    }
}

/// Updates a matched child's properties in-place, evaluating the fast-path [`crate::props::Props::props_eq`]
/// hook to skip redundant updates and subtree re-renders.
fn update_matched_child(
    arena: &mut Arena,
    node_id: NodeId,
    new_props: Box<dyn Any>,
    dirty_cell: &Cell<bool>,
) {
    let props_equal = {
        let node = match arena.get(node_id) {
            Some(n) => n,
            None => return,
        };
        node.instance
            .as_ref()
            .is_some_and(|inst| inst.props_eq_erased(&*new_props))
    };

    if !props_equal {
        // Properties changed: update props in-place
        if let Ok(should_render) = arena.update_props(node_id, new_props, dirty_cell) {
            if should_render.is_yes() {
                if let Some(node) = arena.get_mut(node_id) {
                    node.flags.insert(NodeFlags::DIRTY);
                    dirty_cell.set(true);
                }
            }
        }
    }
    // Fast-path: if props are equal according to props_eq, update_props is skipped.
    // If the node was already marked DIRTY (e.g. state change), it remains DIRTY;
    // if clean, subtree re-rendering is skipped.
}

/// Mounts a new component instance into the arena, setting [`NodeFlags::MOUNTED`],
/// invoking [`crate::component::Component::on_mount`], and recursively mounting its initial rendered children.
pub fn mount_node(
    arena: &mut Arena,
    parent_id: Option<NodeId>,
    depth: u16,
    descriptor: ComponentDescriptor,
    dirty_cell: &Cell<bool>,
) -> NodeId {
    let key = descriptor.key;
    let type_id = descriptor.type_id;
    let props = descriptor.props;
    let factory = descriptor.factory;

    let node_id =
        arena.allocate(|id| InstanceNode::placeholder(id, parent_id, depth, key, type_id));

    if let Some(f) = factory {
        let mut ctx = ComponentContext::with_arena(node_id, dirty_cell, arena);
        let instance = f(props, &mut ctx);
        if let Some(node) = arena.get_mut(node_id) {
            node.instance = Some(instance);
        }
    }

    // 1. Invoke on_mount lifecycle hook
    let _ = arena.on_mount(node_id, dirty_cell);

    // 2. Initial render pass to mount child hierarchy recursively
    let initial_vdom = arena.render(node_id, dirty_cell);
    if let Ok(ui_node) = initial_vdom {
        let child_descriptors = flatten_ui_nodes(vec![ui_node]);
        check_duplicate_explicit_keys(&child_descriptors);
        let mut children = Vec::with_capacity(child_descriptors.len());
        for child_desc in child_descriptors {
            let child_id = mount_node(arena, Some(node_id), depth + 1, child_desc, dirty_cell);
            children.push(child_id);
        }
        if let Some(node) = arena.get_mut(node_id) {
            node.children = children;
        }
    }

    node_id
}

/// Recursively unmounts a node and its entire child subtree in post-order (children before parent).
///
/// For each node:
/// 1. Recursively unmounts all children first.
/// 2. Invokes [`crate::component::Component::on_unmount`].
/// 3. Clears [`NodeFlags::MOUNTED`].
/// 4. Removes the node from the arena, recycling the slot and incrementing its generation.
pub fn unmount_subtree(arena: &mut Arena, root_id: NodeId, dirty_cell: &Cell<bool>) {
    let children = match arena.get(root_id) {
        Some(node) => node.children.clone(),
        None => return,
    };

    // 1. Post-order traversal: unmount children first
    for child_id in children {
        unmount_subtree(arena, child_id, dirty_cell);
    }

    // 2. Invoke on_unmount lifecycle hook
    let _ = arena.on_unmount(root_id, dirty_cell);

    // 3. Clear MOUNTED flag
    if let Some(node) = arena.get_mut(root_id) {
        node.flags.remove(NodeFlags::MOUNTED);
    }

    // 4. Remove node from arena, recycling slot and incrementing generation
    arena.remove(root_id);
}
