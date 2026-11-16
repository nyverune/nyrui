//! Milestone 1 Verification Binary: Generational Arena & Tree Reconciler.
//!
//! Validates the core architectural guarantees specified in **RFC-0001 §13.1**:
//! - Tree mounting, pre-order `on_mount`, and state initialization.
//! - In-place property updates and local instance state preservation.
//! - Keyed sibling matching and state tracking across permutations (`[A, B, C] -> [C, A, B]`).
//! - Element removal, post-order `on_unmount`, and generational ABA protection.
//! - Component type replacement under an identical key.
//! - Positional node isolation across distinct conditional `scope` IDs.
//! - Recursive post-order child-first destruction.

use nyrui_core::component::{Component, ComponentContext, RenderContext, ShouldRender};
use nyrui_core::id::{Key, NodeKey};
use nyrui_core::node::UiNode;
use nyrui_core::node::component::ComponentDescriptor;
use nyrui_core::props::{Props, SharedStr};
use nyrui_core::runtime::arena::{Arena, NodeFlags};
use nyrui_core::runtime::{mount_node, reconcile_children, unmount_subtree};

use std::cell::{Cell, RefCell};
use std::rc::Rc;

// -----------------------------------------------------------------------------
// Component Definitions
// -----------------------------------------------------------------------------

type TraceLog = Rc<RefCell<Vec<String>>>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct CounterProps {
    label: SharedStr,
    step: i32,
}

impl Props for CounterProps {
    fn props_eq(&self, other: &Self) -> bool {
        self.label == other.label && self.step == other.step
    }
}

struct CounterComponent {
    label: SharedStr,
    count: i32,
    trace: Option<TraceLog>,
}

impl Component for CounterComponent {
    type Props = CounterProps;
    type Message = i32;

    fn create(props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self {
            label: props.label.clone(),
            count: props.step * 10,
            trace: None,
        }
    }

    fn update_props(
        &mut self,
        new_props: &Self::Props,
        _ctx: &mut ComponentContext<Self>,
    ) -> ShouldRender {
        self.label = new_props.label.clone();
        ShouldRender::Yes
    }

    fn update(&mut self, msg: Self::Message, _ctx: &mut ComponentContext<Self>) -> ShouldRender {
        self.count += msg;
        ShouldRender::Yes
    }

    fn on_mount(&mut self, _ctx: &mut ComponentContext<Self>) {
        if let Some(trace) = &self.trace {
            trace.borrow_mut().push(format!("{}.on_mount", self.label));
        }
    }

    fn on_unmount(&mut self, _ctx: &mut ComponentContext<Self>) {
        if let Some(trace) = &self.trace {
            trace
                .borrow_mut()
                .push(format!("{}.on_unmount", self.label));
        }
    }

    fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        UiNode::Empty
    }
}

fn make_counter(key: impl Into<Key>, label: &'static str, step: i32) -> UiNode {
    UiNode::Component(ComponentDescriptor::of::<CounterComponent>(
        CounterProps {
            label: SharedStr::from(label),
            step,
        },
        NodeKey::Explicit(key.into()),
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DisplayProps {
    text: SharedStr,
}

impl Props for DisplayProps {}

struct DisplayComponent {
    text: SharedStr,
}

impl Component for DisplayComponent {
    type Props = DisplayProps;
    type Message = ();

    fn create(props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self {
            text: props.text.clone(),
        }
    }

    fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        UiNode::Empty
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InputProps {
    placeholder: SharedStr,
}

impl Props for InputProps {}

struct InputComponent {
    buffer: String,
}

impl Component for InputComponent {
    type Props = InputProps;
    type Message = String;

    fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self {
            buffer: String::new(),
        }
    }

    fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        UiNode::Empty
    }
}

type ChildBuilder = Rc<dyn Fn() -> UiNode>;

#[derive(Clone)]
struct TracedProps {
    name: &'static str,
    trace: TraceLog,
    child_builder: Option<ChildBuilder>,
}

impl std::fmt::Debug for TracedProps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TracedProps")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Props for TracedProps {
    fn props_eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

struct TracedComponent {
    name: &'static str,
    trace: TraceLog,
    child_builder: Option<ChildBuilder>,
}

impl Component for TracedComponent {
    type Props = TracedProps;
    type Message = ();

    fn create(props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self {
            name: props.name,
            trace: props.trace.clone(),
            child_builder: props.child_builder.clone(),
        }
    }

    fn on_mount(&mut self, _ctx: &mut ComponentContext<Self>) {
        self.trace
            .borrow_mut()
            .push(format!("{}.on_mount", self.name));
    }

    fn on_unmount(&mut self, _ctx: &mut ComponentContext<Self>) {
        self.trace
            .borrow_mut()
            .push(format!("{}.on_unmount", self.name));
    }

    fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        match &self.child_builder {
            Some(builder) => builder(),
            None => UiNode::Empty,
        }
    }
}

fn make_traced(
    key: impl Into<Key>,
    name: &'static str,
    trace: &TraceLog,
    child_builder: Option<ChildBuilder>,
) -> UiNode {
    UiNode::Component(ComponentDescriptor::of::<TracedComponent>(
        TracedProps {
            name,
            trace: trace.clone(),
            child_builder,
        },
        NodeKey::Explicit(key.into()),
    ))
}

// -----------------------------------------------------------------------------
// Milestone 1 Verification Verification Suite
// -----------------------------------------------------------------------------

fn main() {
    println!("========================================================================");
    println!(" 🚀 Nyrui Milestone 1 Verification: Generational Arena & Reconciler");
    println!("========================================================================");

    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    // -------------------------------------------------------------------------
    // 1. Initial Tree Mounting & State Initialization
    // -------------------------------------------------------------------------
    print!("[1/7] Mounting initial component hierarchy and verifying state... ");
    let tree = vec![
        make_counter("c1", "Counter 1", 1),
        make_counter("c2", "Counter 2", 2),
    ];
    let res1 = reconcile_children(&mut arena, None, 0, &[], tree, &dirty_cell);
    assert_eq!(res1.children.len(), 2);
    assert_eq!(res1.newly_mounted.len(), 2);
    let id_c1 = res1.children[0];
    let _id_c2 = res1.children[1];

    let node1 = arena.get(id_c1).expect("node 1 must exist");
    assert!(node1.flags.contains(NodeFlags::MOUNTED));
    arena
        .with_instance(id_c1, |inst, _a| {
            let concrete = inst
                .as_any()
                .downcast_ref::<CounterComponent>()
                .expect("downcast to counter");
            assert_eq!(concrete.count, 10);
            assert_eq!(concrete.label.as_str(), "Counter 1");
        })
        .expect("with_instance succeeds");
    println!("OK");

    // -------------------------------------------------------------------------
    // 2. In-place Props Update & Local State Preservation
    // -------------------------------------------------------------------------
    print!("[2/7] Updating props while preserving mutated local state... ");
    // Mutate internal state on node 1: count += 42 => 52
    arena
        .with_instance_mut(id_c1, |inst, _a| {
            let concrete = inst
                .as_any_mut()
                .downcast_mut::<CounterComponent>()
                .expect("downcast to counter");
            concrete.count = 52;
        })
        .expect("with_instance_mut succeeds");

    // Clear dirty flags from initial frame
    for &id in &res1.children {
        arena
            .get_mut(id)
            .expect("node exists")
            .flags
            .remove(NodeFlags::DIRTY);
    }

    // Reconcile with updated label prop
    let updated_tree = vec![
        make_counter("c1", "Counter 1 (Renamed)", 1),
        make_counter("c2", "Counter 2", 2),
    ];
    let res2 = reconcile_children(
        &mut arena,
        None,
        0,
        &res1.children,
        updated_tree,
        &dirty_cell,
    );
    assert_eq!(res2.children, res1.children);
    assert!(res2.unmounted.is_empty());

    // Verify label changed but local state (52) was preserved intact
    arena
        .with_instance(id_c1, |inst, _a| {
            let concrete = inst
                .as_any()
                .downcast_ref::<CounterComponent>()
                .expect("downcast to counter");
            assert_eq!(concrete.label.as_str(), "Counter 1 (Renamed)");
            assert_eq!(concrete.count, 52);
        })
        .expect("with_instance succeeds");
    println!("OK");

    // -------------------------------------------------------------------------
    // 3. Keyed Sibling Reordering ([A, B, C] -> [C, A, B])
    // -------------------------------------------------------------------------
    print!("[3/7] Reordering keyed list ([A, B, C] -> [C, A, B])... ");
    let mut arena_list = Arena::new();
    let initial_items = vec![
        make_counter("a", "Item A", 10),
        make_counter("b", "Item B", 20),
        make_counter("c", "Item C", 30),
    ];
    let r_list1 = reconcile_children(&mut arena_list, None, 0, &[], initial_items, &dirty_cell);
    let id_a = r_list1.children[0];
    let id_b = r_list1.children[1];
    let id_c = r_list1.children[2];

    // Assign custom states
    arena_list
        .with_instance_mut(id_a, |i, _| {
            i.as_any_mut()
                .downcast_mut::<CounterComponent>()
                .expect("downcast")
                .count = 111;
        })
        .expect("mut succeeds");
    arena_list
        .with_instance_mut(id_b, |i, _| {
            i.as_any_mut()
                .downcast_mut::<CounterComponent>()
                .expect("downcast")
                .count = 222;
        })
        .expect("mut succeeds");
    arena_list
        .with_instance_mut(id_c, |i, _| {
            i.as_any_mut()
                .downcast_mut::<CounterComponent>()
                .expect("downcast")
                .count = 333;
        })
        .expect("mut succeeds");

    // Clear dirty flags
    for &id in &r_list1.children {
        arena_list
            .get_mut(id)
            .expect("node exists")
            .flags
            .remove(NodeFlags::DIRTY);
    }

    // Permute: [C, A, B]
    let permuted = vec![
        make_counter("c", "Item C", 30),
        make_counter("a", "Item A", 10),
        make_counter("b", "Item B", 20),
    ];
    let r_list2 = reconcile_children(
        &mut arena_list,
        None,
        0,
        &r_list1.children,
        permuted,
        &dirty_cell,
    );
    assert_eq!(r_list2.children, vec![id_c, id_a, id_b]);
    assert!(r_list2.unmounted.is_empty());
    assert!(r_list2.newly_mounted.is_empty());

    // Verify state followed the keys to their exact positions
    arena_list
        .with_instance(r_list2.children[0], |i, _| {
            assert_eq!(
                i.as_any()
                    .downcast_ref::<CounterComponent>()
                    .expect("downcast")
                    .count,
                333
            );
        })
        .expect("inspect succeeds");
    arena_list
        .with_instance(r_list2.children[1], |i, _| {
            assert_eq!(
                i.as_any()
                    .downcast_ref::<CounterComponent>()
                    .expect("downcast")
                    .count,
                111
            );
        })
        .expect("inspect succeeds");
    arena_list
        .with_instance(r_list2.children[2], |i, _| {
            assert_eq!(
                i.as_any()
                    .downcast_ref::<CounterComponent>()
                    .expect("downcast")
                    .count,
                222
            );
        })
        .expect("inspect succeeds");
    println!("OK");

    // -------------------------------------------------------------------------
    // 4. Middle & Edge Removal with Generational Bump
    // -------------------------------------------------------------------------
    print!("[4/7] Removing middle & edge elements and verifying generational bump... ");
    let mut arena_rem = Arena::new();
    let full_set = vec![
        make_counter("1", "One", 1),
        make_counter("2", "Two", 2),
        make_counter("3", "Three", 3),
        make_counter("4", "Four", 4),
        make_counter("5", "Five", 5),
    ];
    let rem_res1 = reconcile_children(&mut arena_rem, None, 0, &[], full_set, &dirty_cell);
    let id_1 = rem_res1.children[0];
    let id_3 = rem_res1.children[2];
    let id_5 = rem_res1.children[4];

    // Retain only [Two, Four] (removes edge 1, middle 3, edge 5)
    let reduced_set = vec![make_counter("2", "Two", 2), make_counter("4", "Four", 4)];
    let rem_res2 = reconcile_children(
        &mut arena_rem,
        None,
        0,
        &rem_res1.children,
        reduced_set,
        &dirty_cell,
    );
    assert_eq!(rem_res2.children.len(), 2);
    assert_eq!(rem_res2.unmounted.len(), 3);
    assert!(rem_res2.unmounted.contains(&id_1));
    assert!(rem_res2.unmounted.contains(&id_3));
    assert!(rem_res2.unmounted.contains(&id_5));

    // Stale references must fail lookup
    assert!(!arena_rem.contains(id_1));
    assert!(!arena_rem.contains(id_3));
    assert!(!arena_rem.contains(id_5));

    // Allocate a new node to verify generational protection
    let rem_res3 = reconcile_children(
        &mut arena_rem,
        None,
        0,
        &rem_res2.children,
        vec![
            make_counter("2", "Two", 2),
            make_counter("4", "Four", 4),
            make_counter("6", "Six", 6),
        ],
        &dirty_cell,
    );
    let id_6 = rem_res3.children[2];
    assert!(arena_rem.contains(id_6));
    assert!(!arena_rem.contains(id_5));
    println!("OK");

    // -------------------------------------------------------------------------
    // 5. Component Type Replacement with Identical Key
    // -------------------------------------------------------------------------
    print!("[5/7] Replacing component type under same key and discarding old state... ");
    let mut arena_type = Arena::new();
    let comp_a = make_counter("shared", "Counter", 100);
    let tr1 = reconcile_children(&mut arena_type, None, 0, &[], vec![comp_a], &dirty_cell);
    let id_old = tr1.children[0];

    // Mutate state on counter
    arena_type
        .with_instance_mut(id_old, |i, _| {
            i.as_any_mut()
                .downcast_mut::<CounterComponent>()
                .expect("downcast")
                .count = 9999;
        })
        .expect("mut succeeds");

    // Reconcile with DisplayComponent under same key
    let comp_b = UiNode::Component(ComponentDescriptor::of::<DisplayComponent>(
        DisplayProps {
            text: SharedStr::from("Display Content"),
        },
        NodeKey::Explicit(Key::from("shared")),
    ));
    let tr2 = reconcile_children(
        &mut arena_type,
        None,
        0,
        &tr1.children,
        vec![comp_b],
        &dirty_cell,
    );
    let id_new = tr2.children[0];

    assert_ne!(id_old, id_new);
    assert_eq!(tr2.unmounted, vec![id_old]);
    assert_eq!(tr2.newly_mounted, vec![id_new]);
    assert!(!arena_type.contains(id_old));
    assert!(arena_type.contains(id_new));

    // Verify new component has fresh state and downcasting to old component fails
    arena_type
        .with_instance(id_new, |i, _| {
            assert!(i.as_any().downcast_ref::<CounterComponent>().is_none());
            let disp = i
                .as_any()
                .downcast_ref::<DisplayComponent>()
                .expect("downcast to display");
            assert_eq!(disp.text.as_str(), "Display Content");
        })
        .expect("inspect succeeds");
    println!("OK");

    // -------------------------------------------------------------------------
    // 6. Positional Nodes Across Distinct Conditional Scopes
    // -------------------------------------------------------------------------
    print!("[6/7] Verifying positional node isolation across if/else scopes... ");
    let mut arena_pos = Arena::new();

    // Branch 0 (if): Positional (scope: 0, index: 0)
    let branch_0 = UiNode::Component(ComponentDescriptor::of::<InputComponent>(
        InputProps {
            placeholder: SharedStr::from("Input 0"),
        },
        NodeKey::positional(0, 0),
    ));
    let pr1 = reconcile_children(&mut arena_pos, None, 0, &[], vec![branch_0], &dirty_cell);
    let id_pos0 = pr1.children[0];

    // User types into branch 0
    arena_pos
        .with_instance_mut(id_pos0, |i, _| {
            i.as_any_mut()
                .downcast_mut::<InputComponent>()
                .expect("downcast")
                .buffer = "User text in Branch 0".to_string();
        })
        .expect("mut succeeds");

    // Branch 1 (else): Positional (scope: 1, index: 0)
    let branch_1 = UiNode::Component(ComponentDescriptor::of::<InputComponent>(
        InputProps {
            placeholder: SharedStr::from("Input 1"),
        },
        NodeKey::positional(1, 0),
    ));
    let pr2 = reconcile_children(
        &mut arena_pos,
        None,
        0,
        &pr1.children,
        vec![branch_1],
        &dirty_cell,
    );
    let id_pos1 = pr2.children[0];

    assert_ne!(id_pos0, id_pos1);
    assert_eq!(pr2.unmounted, vec![id_pos0]);
    assert_eq!(pr2.newly_mounted, vec![id_pos1]);
    assert!(!arena_pos.contains(id_pos0));

    // Branch 1 buffer is empty, not contaminated by branch 0
    arena_pos
        .with_instance(id_pos1, |i, _| {
            let input = i
                .as_any()
                .downcast_ref::<InputComponent>()
                .expect("downcast");
            assert_eq!(input.buffer, "");
        })
        .expect("inspect succeeds");
    println!("OK");

    // -------------------------------------------------------------------------
    // 7. Post-Order Recursive Subtree Unmounting
    // -------------------------------------------------------------------------
    print!("[7/7] Verifying pre-order mounting and strict post-order unmounting... ");
    let mut arena_hier = Arena::new();
    let trace: TraceLog = Rc::new(RefCell::new(Vec::new()));

    let trace_gc = trace.clone();
    let gc_builder = Rc::new(move || make_traced("gc", "Grandchild", &trace_gc, None));

    let trace_c = trace.clone();
    let c_builder = Rc::new(move || make_traced("c", "Child", &trace_c, Some(gc_builder.clone())));

    let parent_node = make_traced("p", "Parent", &trace, Some(c_builder));
    let descriptor = match parent_node {
        UiNode::Component(d) => d,
        _ => unreachable!(),
    };

    let p_id = mount_node(&mut arena_hier, None, 0, descriptor, &dirty_cell);
    assert_eq!(
        *trace.borrow(),
        vec!["Parent.on_mount", "Child.on_mount", "Grandchild.on_mount"]
    );

    // Unmount root of hierarchy
    unmount_subtree(&mut arena_hier, p_id, &dirty_cell);
    assert_eq!(
        *trace.borrow(),
        vec![
            "Parent.on_mount",
            "Child.on_mount",
            "Grandchild.on_mount",
            "Grandchild.on_unmount",
            "Child.on_unmount",
            "Parent.on_unmount",
        ]
    );
    assert!(!arena_hier.contains(p_id));
    println!("OK");

    println!("========================================================================");
    println!(" 🎉 Milestone 1 Verification Complete: Generational Arena & Reconciler");
    println!(" All 8 RFC-0001 §13.1 verification invariants verified successfully!");
    println!("========================================================================");
}
