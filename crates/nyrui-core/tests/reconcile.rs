//! Integration tests for child reconciliation matching, prefix/suffix fast-path, and key tracking.

use nyrui_core::component::{Component, ComponentContext, RenderContext, ShouldRender};
use nyrui_core::id::{Key, NodeKey};
use nyrui_core::node::UiNode;
use nyrui_core::node::component::ComponentDescriptor;
use nyrui_core::props::{Props, SharedStr};
use nyrui_core::runtime::arena::{Arena, NodeFlags};
use nyrui_core::runtime::reconcile::{check_duplicate_explicit_keys, reconcile_children};

use std::cell::Cell;

// -----------------------------------------------------------------------------
// Test Components
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemProps {
    title: SharedStr,
    value: i32,
}

impl Props for ItemProps {
    fn props_eq(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Debug)]
struct ItemComponent {
    internal_state: i32,
    render_count: u32,
}

impl Component for ItemComponent {
    type Props = ItemProps;
    type Message = i32;

    fn create(props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self {
            internal_state: props.value * 10,
            render_count: 1,
        }
    }

    fn update_props(
        &mut self,
        new_props: &Self::Props,
        _ctx: &mut ComponentContext<Self>,
    ) -> ShouldRender {
        self.render_count += 1;
        if self.internal_state != new_props.value * 10 {
            self.internal_state = new_props.value * 10;
            ShouldRender::Yes
        } else {
            ShouldRender::No
        }
    }

    fn render(&self, props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        UiNode::text(format!("{}: {}", props.title, self.internal_state))
    }
}

fn make_item(key: impl Into<Key>, title: &'static str, value: i32) -> UiNode {
    let k = key.into();
    UiNode::Component(ComponentDescriptor::of::<ItemComponent>(
        ItemProps {
            title: SharedStr::from(title),
            value,
        },
        NodeKey::Explicit(k),
    ))
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[test]
fn prefix_suffix_fast_path_identical_list() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    let v1 = vec![
        make_item("a", "Item A", 1),
        make_item("b", "Item B", 2),
        make_item("c", "Item C", 3),
    ];

    // Initial mount
    let res1 = reconcile_children(&mut arena, None, 0, &[], v1, &dirty_cell);
    assert_eq!(res1.children.len(), 3);
    assert_eq!(res1.newly_mounted.len(), 3);
    assert!(res1.unmounted.is_empty());

    // Clear dirty flags from initial creation
    for &id in &res1.children {
        arena
            .get_mut(id)
            .expect("child exists")
            .flags
            .remove(NodeFlags::DIRTY);
    }

    // Reconcile identical list
    let v2 = vec![
        make_item("a", "Item A", 1),
        make_item("b", "Item B", 2),
        make_item("c", "Item C", 3),
    ];

    let res2 = reconcile_children(&mut arena, None, 0, &res1.children, v2, &dirty_cell);
    assert_eq!(res2.children, res1.children);
    assert!(res2.unmounted.is_empty());
    assert!(res2.newly_mounted.is_empty());

    // Fast-path props_eq bailout: nodes were clean and props equal, so nodes remain clean
    for &id in &res2.children {
        assert!(
            !arena
                .get(id)
                .expect("child exists")
                .flags
                .contains(NodeFlags::DIRTY)
        );
    }
}

#[test]
fn props_eq_bailout_and_update() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    let v1 = vec![make_item("a", "Item A", 1)];
    let res1 = reconcile_children(&mut arena, None, 0, &[], v1, &dirty_cell);
    let id_a = res1.children[0];

    // Clear dirty flag
    arena
        .get_mut(id_a)
        .expect("child exists")
        .flags
        .remove(NodeFlags::DIRTY);

    // 1. Same props -> props_eq bailout (node stays !DIRTY)
    let v2 = vec![make_item("a", "Item A", 1)];
    let res2 = reconcile_children(&mut arena, None, 0, &res1.children, v2, &dirty_cell);
    assert_eq!(res2.children[0], id_a);
    assert!(
        !arena
            .get(id_a)
            .expect("child exists")
            .flags
            .contains(NodeFlags::DIRTY)
    );

    // 2. Changed props -> in-place update (node marked DIRTY)
    let v3 = vec![make_item("a", "Item A Updated", 99)];
    let res3 = reconcile_children(&mut arena, None, 0, &res2.children, v3, &dirty_cell);
    assert_eq!(res3.children[0], id_a);
    assert!(
        arena
            .get(id_a)
            .expect("child exists")
            .flags
            .contains(NodeFlags::DIRTY)
    );
}

#[test]
fn prefix_suffix_prepend_and_append() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    let initial = vec![make_item("b", "Item B", 2)];
    let res1 = reconcile_children(&mut arena, None, 0, &[], initial, &dirty_cell);
    let id_b = res1.children[0];

    // Prepend A and Append C: [B] -> [A, B, C]
    let updated = vec![
        make_item("a", "Item A", 1),
        make_item("b", "Item B", 2),
        make_item("c", "Item C", 3),
    ];
    let res2 = reconcile_children(&mut arena, None, 0, &res1.children, updated, &dirty_cell);
    assert_eq!(res2.children.len(), 3);
    assert_eq!(res2.children[1], id_b); // B preserved in middle
    assert_eq!(res2.newly_mounted.len(), 2);
    assert!(res2.unmounted.is_empty());
}

#[test]
fn middle_slice_keyed_permutation_preserves_instance_state() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    let initial = vec![
        make_item("a", "Item A", 1),
        make_item("b", "Item B", 2),
        make_item("c", "Item C", 3),
    ];
    let res1 = reconcile_children(&mut arena, None, 0, &[], initial, &dirty_cell);
    let id_a = res1.children[0];
    let id_b = res1.children[1];
    let id_c = res1.children[2];

    // Clear dirty flags to simulate completed initial frame
    for &id in &res1.children {
        arena
            .get_mut(id)
            .expect("child exists")
            .flags
            .remove(NodeFlags::DIRTY);
    }

    // Mutate internal state directly on B to ensure state is retained across reorder
    arena
        .with_instance_mut(id_b, |instance, _a| {
            let concrete = instance
                .as_any_mut()
                .downcast_mut::<ItemComponent>()
                .expect("item component downcast");
            concrete.internal_state = 777;
        })
        .expect("with_instance_mut succeeds");

    // Reorder: [A, B, C] -> [C, A, B]
    let permuted = vec![
        make_item("c", "Item C", 3),
        make_item("a", "Item A", 1),
        make_item("b", "Item B", 2),
    ];

    let res2 = reconcile_children(&mut arena, None, 0, &res1.children, permuted, &dirty_cell);
    assert_eq!(res2.children, vec![id_c, id_a, id_b]);
    assert!(res2.unmounted.is_empty());
    assert!(res2.newly_mounted.is_empty());

    // Verify B's state was preserved exactly
    arena
        .with_instance(id_b, |instance, _a| {
            let concrete = instance
                .as_any()
                .downcast_ref::<ItemComponent>()
                .expect("item component downcast");
            assert_eq!(concrete.internal_state, 777);
        })
        .expect("with_instance succeeds");
}

#[test]
fn large_middle_slice_hash_map_lookup() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    // Create 12 items (> 8 to exercise HashMap branch)
    let initial: Vec<UiNode> = (0..12).map(|i| make_item(i as u64, "Item", i)).collect();

    let res1 = reconcile_children(&mut arena, None, 0, &[], initial, &dirty_cell);
    assert_eq!(res1.children.len(), 12);

    // Reverse list: [0..12] -> [11..=0]
    let reversed: Vec<UiNode> = (0..12)
        .rev()
        .map(|i| make_item(i as u64, "Item", i))
        .collect();

    let res2 = reconcile_children(&mut arena, None, 0, &res1.children, reversed, &dirty_cell);
    assert_eq!(res2.children.len(), 12);
    assert!(res2.unmounted.is_empty());
    assert!(res2.newly_mounted.is_empty());

    let mut expected_reversed = res1.children.clone();
    expected_reversed.reverse();
    assert_eq!(res2.children, expected_reversed);
}

#[test]
fn middle_slice_insert_and_delete() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    let initial = vec![
        make_item("a", "Item A", 1),
        make_item("b", "Item B", 2),
        make_item("c", "Item C", 3),
        make_item("d", "Item D", 4),
    ];
    let res1 = reconcile_children(&mut arena, None, 0, &[], initial, &dirty_cell);
    let id_a = res1.children[0];
    let id_b = res1.children[1];
    let id_c = res1.children[2];
    let id_d = res1.children[3];

    // [A, B, C, D] -> [B, E, D]
    // A and C removed, E newly inserted, B and D retained
    let next = vec![
        make_item("b", "Item B", 2),
        make_item("e", "Item E", 5),
        make_item("d", "Item D", 4),
    ];
    let res2 = reconcile_children(&mut arena, None, 0, &res1.children, next, &dirty_cell);
    assert_eq!(res2.children.len(), 3);
    assert_eq!(res2.children[0], id_b);
    assert_eq!(res2.children[2], id_d);
    assert_eq!(res2.unmounted, vec![id_a, id_c]);
    assert_eq!(res2.newly_mounted.len(), 1);
}

#[test]
fn fragment_flattening_in_reconciler() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    let tree = vec![
        UiNode::fragment([make_item("a", "Item A", 1), make_item("b", "Item B", 2)]),
        UiNode::empty(),
        make_item("c", "Item C", 3),
    ];

    let res = reconcile_children(&mut arena, None, 0, &[], tree, &dirty_cell);
    assert_eq!(res.children.len(), 3);
    assert_eq!(res.newly_mounted.len(), 3);
}

#[test]
#[should_panic(expected = "Duplicate explicit key")]
fn duplicate_explicit_key_panics_in_debug() {
    let descriptors = vec![
        ComponentDescriptor::new::<ItemComponent, _>(
            ItemProps {
                title: SharedStr::from("One"),
                value: 1,
            },
            NodeKey::Explicit(Key::from("dup")),
        ),
        ComponentDescriptor::new::<ItemComponent, _>(
            ItemProps {
                title: SharedStr::from("Two"),
                value: 2,
            },
            NodeKey::Explicit(Key::from("dup")),
        ),
    ];

    check_duplicate_explicit_keys(&descriptors);
}
