//! Integration and unit tests for the depth-ordered dirty node scheduler and batch coalescing.

use nyrui_core::component::{Component, ComponentContext, RenderContext};
use nyrui_core::id::{Key, NodeId, NodeKey};
use nyrui_core::node::UiNode;
use nyrui_core::node::component::ComponentDescriptor;
use nyrui_core::props::Props;
use nyrui_core::reactivity::Signal;
use nyrui_core::runtime::arena::{Arena, NodeFlags};
use nyrui_core::runtime::scheduler::{ScheduledNode, Scheduler};
use nyrui_core::runtime::{Runtime, mount_node};

use std::cell::Cell;
use std::collections::BinaryHeap;
use std::rc::Rc;

// -----------------------------------------------------------------------------
// Helper Components
// -----------------------------------------------------------------------------

#[derive(Clone, PartialEq, Debug)]
struct ChildProps {
    val: i32,
    signal: Signal<i32>,
    render_trace: Rc<Cell<usize>>,
}

impl Props for ChildProps {
    fn props_eq(&self, other: &Self) -> bool {
        self.val == other.val
    }
}

struct ChildComponent;

impl Component for ChildComponent {
    type Props = ChildProps;
    type Message = ();

    fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self
    }

    fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
        // Increment render counter
        props.render_trace.set(props.render_trace.get() + 1);
        // Also read the signal directly to create a direct reactive edge to Child as well
        let _ = props.signal.get(ctx);

        UiNode::Text(nyrui_core::node::TextNode::new(format!(
            "Value: {}",
            props.val
        )))
    }
}

// -----------------------------------------------------------------------------
// Unit Tests: ScheduledNode Min-Heap
// -----------------------------------------------------------------------------

#[test]
fn scheduled_node_min_heap_depth_ordering() {
    let mut heap = BinaryHeap::new();

    let id0 = NodeId::new(0, std::num::NonZeroU32::new(1).expect("non-zero"));
    let id1 = NodeId::new(1, std::num::NonZeroU32::new(1).expect("non-zero"));
    let id2 = NodeId::new(2, std::num::NonZeroU32::new(1).expect("non-zero"));
    let id3 = NodeId::new(3, std::num::NonZeroU32::new(1).expect("non-zero"));

    // Push in arbitrary order
    heap.push(ScheduledNode { depth: 3, id: id3 });
    heap.push(ScheduledNode { depth: 0, id: id0 });
    heap.push(ScheduledNode { depth: 2, id: id2 });
    heap.push(ScheduledNode { depth: 1, id: id1 });

    // Must pop in ascending depth order: 0, 1, 2, 3
    assert_eq!(heap.pop().expect("pop item").depth, 0);
    assert_eq!(heap.pop().expect("pop item").depth, 1);
    assert_eq!(heap.pop().expect("pop item").depth, 2);
    assert_eq!(heap.pop().expect("pop item").depth, 3);
    assert!(heap.is_empty());
}

#[test]
fn scheduled_node_same_depth_tie_breaking() {
    let mut heap = BinaryHeap::new();

    let id_low = NodeId::new(5, std::num::NonZeroU32::new(1).expect("non-zero"));
    let id_high = NodeId::new(10, std::num::NonZeroU32::new(1).expect("non-zero"));

    heap.push(ScheduledNode {
        depth: 2,
        id: id_high,
    });
    heap.push(ScheduledNode {
        depth: 2,
        id: id_low,
    });

    // Smaller NodeId pops first for deterministic tie-breaking
    assert_eq!(heap.pop().expect("pop item").id, id_low);
    assert_eq!(heap.pop().expect("pop item").id, id_high);
}

#[test]
fn scheduler_deduplication_prevents_duplicate_entries() {
    let mut scheduler = Scheduler::new();
    let id = NodeId::new(1, std::num::NonZeroU32::new(1).expect("non-zero"));

    assert!(scheduler.enqueue_with_depth(id, 1));
    assert_eq!(scheduler.len(), 1);
    assert!(scheduler.contains(id));

    // Second enqueue of identical node must be rejected
    assert!(!scheduler.enqueue_with_depth(id, 1));
    assert_eq!(scheduler.len(), 1);

    // After pop, it can be re-enqueued
    let popped = scheduler.pop();
    assert_eq!(popped.expect("pop item").id, id);
    assert_eq!(scheduler.len(), 0);
    assert!(!scheduler.contains(id));

    assert!(scheduler.enqueue_with_depth(id, 2));
    assert_eq!(scheduler.len(), 1);
}

// -----------------------------------------------------------------------------
// Core Acceptance Criterion: Parent + Child Simultaneous Invalidation
// -----------------------------------------------------------------------------

#[test]
fn parent_child_simultaneous_invalidation_renders_child_only_once() {
    let mut runtime = Runtime::new();

    let parent_trace = Rc::new(Cell::new(0));
    let child_trace = Rc::new(Cell::new(0));

    let sig = runtime.reactive_graph.create_signal(10);

    // Define Parent component whose child tracks child_trace
    struct TrackedParent;
    #[derive(Clone, PartialEq, Debug)]
    struct TrackedProps {
        sig: Signal<i32>,
        parent_trace: Rc<Cell<usize>>,
        child_trace: Rc<Cell<usize>>,
    }
    impl Props for TrackedProps {}

    impl Component for TrackedParent {
        type Props = TrackedProps;
        type Message = ();

        fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
            Self
        }

        fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
            props.parent_trace.set(props.parent_trace.get() + 1);
            let val = props.sig.get(ctx);

            UiNode::Component(ComponentDescriptor::of::<TrackedChild>(
                TrackedChildProps {
                    val,
                    sig: props.sig,
                    child_trace: props.child_trace.clone(),
                },
                NodeKey::Explicit(Key::from("tracked_child")),
            ))
        }
    }

    #[derive(Clone, PartialEq, Debug)]
    struct TrackedChildProps {
        val: i32,
        sig: Signal<i32>,
        child_trace: Rc<Cell<usize>>,
    }
    impl Props for TrackedChildProps {
        fn props_eq(&self, other: &Self) -> bool {
            self.val == other.val
        }
    }

    struct TrackedChild;
    impl Component for TrackedChild {
        type Props = TrackedChildProps;
        type Message = ();

        fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
            Self
        }

        fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
            props.child_trace.set(props.child_trace.get() + 1);
            // Child also directly observes the signal
            let val = props.sig.get(ctx);
            UiNode::Text(nyrui_core::node::TextNode::new(format!("Child Val: {val}")))
        }
    }

    // 1. Mount root tree (depth 0: Parent, depth 1: Child)
    let root_id = runtime.mount_root(ComponentDescriptor::of::<TrackedParent>(
        TrackedProps {
            sig,
            parent_trace: parent_trace.clone(),
            child_trace: child_trace.clone(),
        },
        NodeKey::Explicit(Key::from("root")),
    ));

    // Verify initial mount rendered each exactly once
    assert_eq!(parent_trace.get(), 1, "Parent rendered once on mount");
    assert_eq!(child_trace.get(), 1, "Child rendered once on mount");

    let root_node = runtime.arena.get(root_id).expect("root exists");
    assert_eq!(root_node.depth, 0);
    assert_eq!(root_node.children.len(), 1);
    let child_id = root_node.children[0];
    let child_node = runtime.arena.get(child_id).expect("child exists");
    assert_eq!(child_node.depth, 1);

    // 2. Mutate signal read by BOTH Parent and Child
    // This marks BOTH Parent (depth 0) and Child (depth 1) dirty simultaneously in graph.dirty_nodes
    runtime.batch(|cx| {
        sig.set(cx, 42);
    });

    // 3. Verification:
    // With top-down depth ordering, Parent (depth 0) reconciles FIRST, updating Child's props.
    // When Child (depth 1) is then popped, it renders with the updated props.
    // Child renders EXACTLY ONCE during the flush pass! Total renders: 2 (1 mount + 1 re-render).
    assert_eq!(
        parent_trace.get(),
        2,
        "Parent must re-render exactly once during batch flush"
    );
    assert_eq!(
        child_trace.get(),
        2,
        "Child must re-render exactly once during batch flush (never twice)"
    );

    // 4. Multiple mutations in another batch
    runtime.batch(|cx| {
        sig.set(cx, 100);
        sig.set(cx, 200);
        sig.set(cx, 300);
    });

    assert_eq!(parent_trace.get(), 3);
    assert_eq!(child_trace.get(), 3);
}

// -----------------------------------------------------------------------------
// Unmounted Descendant Skip Test
// -----------------------------------------------------------------------------

#[test]
fn unmounted_dirty_descendant_is_skipped_during_flush() {
    let mut runtime = Runtime::new();

    let parent_trace = Rc::new(Cell::new(0));
    let child_trace = Rc::new(Cell::new(0));

    let show_child_sig = runtime.reactive_graph.create_signal(true);
    let data_sig = runtime.reactive_graph.create_signal(10);

    struct ConditionalParent;
    #[derive(Clone, PartialEq, Debug)]
    struct CondProps {
        show_child: Signal<bool>,
        data: Signal<i32>,
        parent_trace: Rc<Cell<usize>>,
        child_trace: Rc<Cell<usize>>,
    }
    impl Props for CondProps {}

    impl Component for ConditionalParent {
        type Props = CondProps;
        type Message = ();

        fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
            Self
        }

        fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
            props.parent_trace.set(props.parent_trace.get() + 1);
            let show = props.show_child.get(ctx);

            if show {
                UiNode::Component(ComponentDescriptor::of::<ChildComponent>(
                    ChildProps {
                        val: props.data.get(ctx),
                        signal: props.data,
                        render_trace: props.child_trace.clone(),
                    },
                    NodeKey::Explicit(Key::from("child")),
                ))
            } else {
                UiNode::Empty
            }
        }
    }

    // Mount initially with show_child = true
    let root_id = runtime.mount_root(ComponentDescriptor::of::<ConditionalParent>(
        CondProps {
            show_child: show_child_sig,
            data: data_sig,
            parent_trace: parent_trace.clone(),
            child_trace: child_trace.clone(),
        },
        NodeKey::Explicit(Key::from("cond_root")),
    ));

    assert_eq!(parent_trace.get(), 1);
    assert_eq!(child_trace.get(), 1);

    let child_id = runtime.arena.get(root_id).expect("root exists").children[0];
    assert!(runtime.arena.contains(child_id));

    // Simultaneously invalidate show_child (Parent will unmount Child) AND data_sig (Child is dirty)
    runtime.batch(|cx| {
        show_child_sig.set(cx, false);
        data_sig.set(cx, 999);
    });

    // Parent reconciles and unmounts Child.
    // Child was dirty in the scheduler, but when popped, arena.contains(child_id) is false!
    // Child render must NOT be invoked!
    assert_eq!(parent_trace.get(), 2);
    assert_eq!(
        child_trace.get(),
        1,
        "Unmounted child must not be re-rendered"
    );
    assert!(!runtime.arena.contains(child_id));
}

// -----------------------------------------------------------------------------
// Clean / Non-dirty Node Skip Test
// -----------------------------------------------------------------------------

#[test]
fn clean_node_is_skipped_if_dirty_flag_cleared() {
    let mut arena = Arena::new();
    let graph = Rc::new(nyrui_core::reactivity::ReactiveGraph::new());
    arena.set_reactive_graph(graph.clone());
    let mut scheduler = Scheduler::new();
    let dirty_cell = Cell::new(false);

    let render_count = Rc::new(Cell::new(0));

    struct SimpleComp;
    #[derive(Clone, PartialEq, Debug)]
    struct SimpleProps(Rc<Cell<usize>>);
    impl Props for SimpleProps {}

    impl Component for SimpleComp {
        type Props = SimpleProps;
        type Message = ();
        fn create(_: &Self::Props, _: &mut ComponentContext<Self>) -> Self {
            Self
        }
        fn render(&self, props: &Self::Props, _: &RenderContext<Self>) -> UiNode {
            props.0.set(props.0.get() + 1);
            UiNode::Empty
        }
    }

    let node_id = mount_node(
        &mut arena,
        None,
        0,
        ComponentDescriptor::of::<SimpleComp>(
            SimpleProps(render_count.clone()),
            NodeKey::Explicit(Key::from("simple")),
        ),
        &dirty_cell,
    );

    assert_eq!(render_count.get(), 1);

    // Enqueue node
    scheduler.enqueue(&arena, node_id);
    assert_eq!(scheduler.len(), 1);

    // Manually clear dirty flag
    arena
        .get_mut(node_id)
        .expect("node exists")
        .flags
        .remove(NodeFlags::DIRTY);

    // Flush: node should be skipped because !DIRTY
    let rendered = scheduler.flush(&mut arena, &graph, &dirty_cell);
    assert_eq!(rendered, 0);
    assert_eq!(render_count.get(), 1);
}

// -----------------------------------------------------------------------------
// Three-Level Hierarchy Depth-Order Test
// -----------------------------------------------------------------------------

#[test]
fn three_level_hierarchy_reconciles_in_depth_order() {
    let mut runtime = Runtime::new();

    let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
    let sig = runtime.reactive_graph.create_signal(0);

    struct LevelComp;
    #[derive(Clone, PartialEq, Debug)]
    struct LevelProps {
        level: u8,
        sig: Signal<i32>,
        trace: Rc<std::cell::RefCell<Vec<u8>>>,
    }
    impl Props for LevelProps {}

    impl Component for LevelComp {
        type Props = LevelProps;
        type Message = ();
        fn create(_: &Self::Props, _: &mut ComponentContext<Self>) -> Self {
            Self
        }
        fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
            props.trace.borrow_mut().push(props.level);
            let _ = props.sig.get(ctx);

            if props.level < 2 {
                UiNode::Component(ComponentDescriptor::of::<LevelComp>(
                    LevelProps {
                        level: props.level + 1,
                        sig: props.sig,
                        trace: props.trace.clone(),
                    },
                    NodeKey::Explicit(Key::from(format!("level_{}", props.level + 1))),
                ))
            } else {
                UiNode::Empty
            }
        }
    }

    // Mount hierarchy: Level 0 (depth 0) -> Level 1 (depth 1) -> Level 2 (depth 2)
    runtime.mount_root(ComponentDescriptor::of::<LevelComp>(
        LevelProps {
            level: 0,
            sig,
            trace: trace.clone(),
        },
        NodeKey::Explicit(Key::from("root")),
    ));

    // Clear mount trace
    trace.borrow_mut().clear();

    // Mutate signal read by all three levels
    runtime.batch(|cx| {
        sig.set(cx, 42);
    });

    // Trace must show exact depth order: 0, 1, 2
    assert_eq!(*trace.borrow(), vec![0, 1, 2]);
}

// -----------------------------------------------------------------------------
// Scheduler Infinite Loop Guard Test
// -----------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Exceeded maximum scheduler flush iterations")]
fn scheduler_detects_infinite_render_loop_and_panics() {
    let mut arena = Arena::new();
    let graph = Rc::new(nyrui_core::reactivity::ReactiveGraph::new());
    arena.set_reactive_graph(graph.clone());
    let mut scheduler = Scheduler::new();
    scheduler.set_max_passes(10);
    let dirty_cell = Cell::new(false);

    let sig = graph.create_signal(0);

    struct LoopingComp;
    #[derive(Clone, PartialEq, Debug)]
    struct LoopingProps(Signal<i32>);
    impl Props for LoopingProps {}

    impl Component for LoopingComp {
        type Props = LoopingProps;
        type Message = ();
        fn create(_: &Self::Props, _: &mut ComponentContext<Self>) -> Self {
            Self
        }
        fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
            let val = props.0.get(ctx);
            // Intentionally mutate signal during render to cause infinite loop
            props.0.set(ctx, val + 1);
            UiNode::Empty
        }
    }

    let _ = mount_node(
        &mut arena,
        None,
        0,
        ComponentDescriptor::of::<LoopingComp>(
            LoopingProps(sig),
            NodeKey::Explicit(Key::from("loop")),
        ),
        &dirty_cell,
    );

    // This flush must panic when it exceeds 10 passes
    let _ = scheduler.flush(&mut arena, &graph, &dirty_cell);
}
