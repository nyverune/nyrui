//! Integration tests for Arena, InstanceNode, ErasedComponent, and Take-and-Restore execution.

use nyrui_core::component::{Component, ComponentContext, RenderContext, ShouldRender};
use nyrui_core::id::{Key, NodeKey};
use nyrui_core::node::UiNode;
use nyrui_core::props::{Props, SharedStr};
use nyrui_core::runtime::arena::{Arena, ArenaError, InstanceNode, NodeFlags};

use std::cell::Cell;

// -----------------------------------------------------------------------------
// Test Components
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct LabelProps {
    text: SharedStr,
}

impl Props for LabelProps {
    fn props_eq(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Debug)]
struct LabelComponent {
    render_count: u32,
}

impl Component for LabelComponent {
    type Props = LabelProps;
    type Message = ();

    fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self { render_count: 0 }
    }

    fn render(&self, props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        UiNode::text(props.text.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CounterProps {
    initial: i32,
}

impl Props for CounterProps {}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CounterMsg {
    Add(i32),
    Reset,
}

#[derive(Debug)]
struct CounterComponent {
    value: i32,
    mounted: bool,
    unmounted: bool,
}

impl Component for CounterComponent {
    type Props = CounterProps;
    type Message = CounterMsg;

    fn create(props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self {
            value: props.initial,
            mounted: false,
            unmounted: false,
        }
    }

    fn update(&mut self, msg: Self::Message, _ctx: &mut ComponentContext<Self>) -> ShouldRender {
        match msg {
            CounterMsg::Add(n) => {
                self.value += n;
                ShouldRender::Yes
            }
            CounterMsg::Reset => {
                self.value = 0;
                ShouldRender::Yes
            }
        }
    }

    fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        UiNode::text(format!("Count: {}", self.value))
    }

    fn on_mount(&mut self, _ctx: &mut ComponentContext<Self>) {
        self.mounted = true;
    }

    fn on_unmount(&mut self, _ctx: &mut ComponentContext<Self>) {
        self.unmounted = true;
    }
}

// -----------------------------------------------------------------------------
// Arena Tests
// -----------------------------------------------------------------------------

#[test]
fn arena_allocation_and_generational_protection() {
    let mut arena = Arena::new();
    assert!(arena.is_empty());
    assert_eq!(arena.len(), 0);

    // 1. Allocate first node
    let id1 = arena.allocate(|id| {
        InstanceNode::new(
            id,
            None,
            0,
            NodeKey::Explicit(Key::from("node-1")),
            CounterComponent {
                value: 10,
                mounted: false,
                unmounted: false,
            },
            CounterProps { initial: 10 },
        )
    });

    assert_eq!(arena.len(), 1);
    assert!(arena.contains(id1));
    assert_eq!(arena.get(id1).expect("node exists").id, id1);

    // 2. Remove node
    let removed = arena.remove(id1).expect("remove succeeds");
    assert_eq!(removed.id, id1);
    assert_eq!(arena.len(), 0);
    assert!(!arena.contains(id1));
    assert!(arena.get(id1).is_none());

    // 3. Allocate second node - reuses slot with incremented generation
    let id2 = arena.allocate(|id| {
        InstanceNode::new(
            id,
            None,
            0,
            NodeKey::Explicit(Key::from("node-2")),
            LabelComponent { render_count: 0 },
            LabelProps {
                text: SharedStr::from("Hello"),
            },
        )
    });

    assert_eq!(id2.index, id1.index);
    assert_ne!(id2.generation, id1.generation);
    assert!(id2.generation > id1.generation);

    // Verify stale reference id1 is completely rejected
    assert!(arena.get(id1).is_none());
    assert!(arena.get_mut(id1).is_none());
    assert!(arena.remove(id1).is_none());

    // Verify active reference id2 works
    assert!(arena.contains(id2));
    assert_eq!(arena.get(id2).expect("id2 exists").id, id2);
}

#[test]
fn arena_lifecycle_execution_with_take_and_restore() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    let id = arena.allocate(|id| {
        InstanceNode::new(
            id,
            None,
            0,
            NodeKey::Positional { scope: 0, index: 0 },
            CounterComponent {
                value: 5,
                mounted: false,
                unmounted: false,
            },
            CounterProps { initial: 5 },
        )
    });

    // 1. Mount
    arena.on_mount(id, &dirty_cell).expect("on_mount succeeds");

    // 2. Render
    let vdom = arena.render(id, &dirty_cell).expect("render succeeds");
    match vdom {
        UiNode::Text(t) => assert_eq!(t.content.as_str(), "Count: 5"),
        other => panic!("expected text, got {other:?}"),
    }

    // 3. Update via message
    let should_render = arena
        .update(id, Box::new(CounterMsg::Add(7)), &dirty_cell)
        .expect("update succeeds");
    assert_eq!(should_render, ShouldRender::Yes);

    // Re-render to observe new value
    let vdom2 = arena.render(id, &dirty_cell).expect("render succeeds");
    match vdom2 {
        UiNode::Text(t) => assert_eq!(t.content.as_str(), "Count: 12"),
        other => panic!("expected text, got {other:?}"),
    }

    // Reset via message
    let should_render_reset = arena
        .update(id, Box::new(CounterMsg::Reset), &dirty_cell)
        .expect("reset succeeds");
    assert_eq!(should_render_reset, ShouldRender::Yes);

    let vdom3 = arena.render(id, &dirty_cell).expect("render succeeds");
    match vdom3 {
        UiNode::Text(t) => assert_eq!(t.content.as_str(), "Count: 0"),
        other => panic!("expected text, got {other:?}"),
    }

    // 4. Unmount
    arena
        .on_unmount(id, &dirty_cell)
        .expect("on_unmount succeeds");
}

#[test]
fn reentrant_arena_mutation_without_borrow_panics() {
    let mut arena = Arena::new();

    // Node A (controller)
    let node_a = arena.allocate(|id| {
        InstanceNode::new(
            id,
            None,
            0,
            NodeKey::Explicit(Key::from("controller")),
            CounterComponent {
                value: 0,
                mounted: false,
                unmounted: false,
            },
            CounterProps { initial: 0 },
        )
    });

    // Node B (sibling / target)
    let node_b = arena.allocate(|id| {
        InstanceNode::new(
            id,
            Some(node_a),
            1,
            NodeKey::Explicit(Key::from("sibling")),
            CounterComponent {
                value: 100,
                mounted: false,
                unmounted: false,
            },
            CounterProps { initial: 100 },
        )
    });

    // Execute with_instance_mut on Node A
    let child_id = arena
        .with_instance_mut(node_a, |_instance_a, arena_ref| {
            // While Node A's instance is taken out:
            // 1. We can inspect Node A's metadata in the arena
            let node_a_ref = arena_ref.get(node_a).expect("Node A metadata exists");
            assert_eq!(node_a_ref.depth, 0);
            assert!(node_a_ref.instance.is_none()); // Temporarily taken!

            // 2. We can mutate Node B in the arena
            let node_b_ref = arena_ref.get_mut(node_b).expect("Node B exists");
            node_b_ref.flags.insert(NodeFlags::FOCUSED);

            // 3. We can allocate a new child node in the arena
            let new_child = arena_ref.allocate(|id| {
                InstanceNode::new(
                    id,
                    Some(node_a),
                    1,
                    NodeKey::Explicit(Key::from("spawned_child")),
                    LabelComponent { render_count: 1 },
                    LabelProps {
                        text: SharedStr::from("Spawned"),
                    },
                )
            });

            // 4. Attempting to re-borrow Node A detects the in-flight borrow gracefully
            let reborrow_res = arena_ref.with_instance_mut(node_a, |_inst, _a| ());
            assert_eq!(
                reborrow_res,
                Err(ArenaError::ComponentAlreadyBorrowed(node_a))
            );

            new_child
        })
        .expect("with_instance_mut succeeds");

    // After with_instance_mut finishes:
    // Node A is fully restored
    let node_a_after = arena.get(node_a).expect("Node A exists");
    assert!(node_a_after.instance.is_some());

    // Mutations on Node B persisted
    let node_b_after = arena.get(node_b).expect("Node B exists");
    assert!(node_b_after.flags.contains(NodeFlags::FOCUSED));

    // The newly spawned child exists in the arena
    assert!(arena.contains(child_id));
    assert_eq!(arena.len(), 3);
}

#[test]
fn erased_component_downcasting_and_props_equality() {
    let mut arena = Arena::new();
    let id = arena.allocate(|id| {
        InstanceNode::new(
            id,
            None,
            0,
            NodeKey::Positional { scope: 0, index: 0 },
            LabelComponent { render_count: 0 },
            LabelProps {
                text: SharedStr::from("Header"),
            },
        )
    });

    let node = arena.get(id).expect("node exists");
    let instance = node.instance.as_ref().expect("instance exists");

    // 1. Props equality
    let same_props = LabelProps {
        text: SharedStr::from("Header"),
    };
    let diff_props = LabelProps {
        text: SharedStr::from("Footer"),
    };
    let wrong_type_props = CounterProps { initial: 10 };

    assert!(instance.props_eq_erased(&same_props));
    assert!(!instance.props_eq_erased(&diff_props));
    assert!(!instance.props_eq_erased(&wrong_type_props));

    // 2. Concrete downcasting
    let concrete = instance.as_any().downcast_ref::<LabelComponent>();
    assert!(concrete.is_some());
    assert_eq!(concrete.expect("downcast").render_count, 0);
}
