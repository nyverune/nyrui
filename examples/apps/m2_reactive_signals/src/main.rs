//! Milestone 2 Verification Binary: Reactivity & Depth-Ordered Scheduler.
//!
//! Validates the core architectural guarantees specified in **RFC-0001 §7 and §13**:
//! - Fine-grained signal tracking & selective re-rendering without full-tree invalidations.
//! - Derived computations ([`nyrui_core::reactivity::Computed`]) push-pull evaluation & lazy caching.
//! - Diamond dependency graph resolution without glitches (`A -> B, A -> C, (B, C) -> D`).
//! - Min-heap depth-ordered dirty scheduler coalescing simultaneous parent + child mutations.
//! - Clean pruning and render skipping for dirty descendants unmounted by ancestors.
//! - Multi-mutation batch coalescing via [`nyrui_core::runtime::Runtime::batch`].

use nyrui_core::component::{Component, ComponentContext, RenderContext};
use nyrui_core::id::{Key, NodeKey};
use nyrui_core::node::UiNode;
use nyrui_core::node::component::ComponentDescriptor;
use nyrui_core::props::Props;
use nyrui_core::reactivity::Signal;
use nyrui_core::runtime::Runtime;

use std::cell::Cell;
use std::rc::Rc;

// -----------------------------------------------------------------------------
// Component Definitions for Verification Scenarios
// -----------------------------------------------------------------------------

#[derive(Clone, PartialEq, Debug)]
struct LeafProps {
    sig: Signal<i32>,
    render_count: Rc<Cell<usize>>,
}

impl Props for LeafProps {}

struct LeafComponent;

impl Component for LeafComponent {
    type Props = LeafProps;
    type Message = ();

    fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self
    }

    fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
        props.render_count.set(props.render_count.get() + 1);
        let val = props.sig.get(ctx);
        UiNode::Text(nyrui_core::node::TextNode::new(format!("Leaf: {val}")))
    }
}

#[derive(Clone, PartialEq, Debug)]
struct BranchProps {
    sig_a: Signal<i32>,
    sig_b: Signal<i32>,
    trace_a: Rc<Cell<usize>>,
    trace_b: Rc<Cell<usize>>,
}

impl Props for BranchProps {}

struct BranchComponent;

impl Component for BranchComponent {
    type Props = BranchProps;
    type Message = ();

    fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self
    }

    fn render(&self, props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        // Root renders two independent subtrees without reading either signal
        UiNode::Fragment(vec![
            UiNode::Component(ComponentDescriptor::of::<LeafComponent>(
                LeafProps {
                    sig: props.sig_a,
                    render_count: props.trace_a.clone(),
                },
                NodeKey::Explicit(Key::from("branch_a")),
            )),
            UiNode::Component(ComponentDescriptor::of::<LeafComponent>(
                LeafProps {
                    sig: props.sig_b,
                    render_count: props.trace_b.clone(),
                },
                NodeKey::Explicit(Key::from("branch_b")),
            )),
        ])
    }
}

#[derive(Clone, PartialEq, Debug)]
struct ParentChildProps {
    sig: Signal<i32>,
    parent_renders: Rc<Cell<usize>>,
    child_renders: Rc<Cell<usize>>,
}

impl Props for ParentChildProps {}

struct MilestoneParent;

impl Component for MilestoneParent {
    type Props = ParentChildProps;
    type Message = ();

    fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self
    }

    fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
        props.parent_renders.set(props.parent_renders.get() + 1);
        let val = props.sig.get(ctx);

        UiNode::Component(ComponentDescriptor::of::<MilestoneChild>(
            MilestoneChildProps {
                val,
                sig: props.sig,
                child_renders: props.child_renders.clone(),
            },
            NodeKey::Explicit(Key::from("milestone_child")),
        ))
    }
}

#[derive(Clone, PartialEq, Debug)]
struct MilestoneChildProps {
    val: i32,
    sig: Signal<i32>,
    child_renders: Rc<Cell<usize>>,
}

impl Props for MilestoneChildProps {
    fn props_eq(&self, other: &Self) -> bool {
        self.val == other.val
    }
}

struct MilestoneChild;

impl Component for MilestoneChild {
    type Props = MilestoneChildProps;
    type Message = ();

    fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self
    }

    fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
        props.child_renders.set(props.child_renders.get() + 1);
        let _ = props.sig.get(ctx);
        UiNode::Text(nyrui_core::node::TextNode::new(format!(
            "Child value: {}",
            props.val
        )))
    }
}

fn main() {
    println!("=== Nyrui Milestone 2 Verification: Reactivity & Scheduler ===");

    let mut runtime = Runtime::new();

    // -------------------------------------------------------------------------
    // 1. Fine-Grained Reactive Tracking & Selective Re-Rendering
    // -------------------------------------------------------------------------
    print!("[1/6] Verifying fine-grained signal tracking & selective re-rendering... ");
    let sig_a = runtime.reactive_graph.create_signal(100);
    let sig_b = runtime.reactive_graph.create_signal(200);

    let trace_a = Rc::new(Cell::new(0));
    let trace_b = Rc::new(Cell::new(0));

    let root_desc = ComponentDescriptor::of::<BranchComponent>(
        BranchProps {
            sig_a,
            sig_b,
            trace_a: trace_a.clone(),
            trace_b: trace_b.clone(),
        },
        NodeKey::Explicit(Key::from("branch_root")),
    );

    let _root_id = runtime.mount_root(root_desc);
    assert_eq!(trace_a.get(), 1);
    assert_eq!(trace_b.get(), 1);

    // Mutating sig_a must dirty and re-render only Leaf A
    runtime.batch(|cx| {
        sig_a.set(cx, 101);
    });

    assert_eq!(trace_a.get(), 2, "Leaf A must re-render on sig_a change");
    assert_eq!(
        trace_b.get(),
        1,
        "Leaf B must NOT re-render when unaffected sig_a changes"
    );

    // Mutating sig_b must dirty and re-render only Leaf B
    runtime.batch(|cx| {
        sig_b.set(cx, 201);
    });

    assert_eq!(trace_a.get(), 2);
    assert_eq!(trace_b.get(), 2);
    println!("OK");

    // -------------------------------------------------------------------------
    // 2. Derived Computations: Push-Pull & Lazy Evaluation
    // -------------------------------------------------------------------------
    print!("[2/6] Verifying derived Computed<T> push-pull evaluation & lazy caching... ");
    let base = runtime.reactive_graph.create_signal(5);
    let eval_count = Rc::new(Cell::new(0));
    let ec = eval_count.clone();

    let squared = runtime
        .reactive_graph
        .create_computed_with_context(move |g| {
            ec.set(ec.get() + 1);
            let b = base.get(g);
            b * b
        });

    // Computation is lazy: evaluation count is 0 before reading
    assert_eq!(eval_count.get(), 0);

    // First read triggers evaluation
    assert_eq!(squared.get(&runtime.reactive_graph), 25);
    assert_eq!(eval_count.get(), 1);

    // Repeated read reuses cached value without re-evaluating
    assert_eq!(squared.get(&runtime.reactive_graph), 25);
    assert_eq!(eval_count.get(), 1);

    // Mutating signal dirties computed lazily
    base.set(&runtime, 6);
    assert_eq!(
        eval_count.get(),
        1,
        "Push phase must only mark dirty, not re-evaluate"
    );

    // Subsequent read pulls updated value
    assert_eq!(squared.get(&runtime.reactive_graph), 36);
    assert_eq!(eval_count.get(), 2);
    println!("OK");

    // -------------------------------------------------------------------------
    // 3. Diamond Dependency Resolution (Glitch-Free)
    // -------------------------------------------------------------------------
    print!("[3/6] Verifying diamond dependency resolution (A -> B, A -> C, (B, C) -> D)... ");
    let a = runtime.reactive_graph.create_signal(2);
    let b = runtime
        .reactive_graph
        .create_computed_with_context(move |g| a.get(g) * 2);
    let c = runtime
        .reactive_graph
        .create_computed_with_context(move |g| a.get(g) + 10);

    let d_evals = Rc::new(Cell::new(0));
    let de = d_evals.clone();
    let d = runtime
        .reactive_graph
        .create_computed_with_context(move |g| {
            de.set(de.get() + 1);
            b.get(g) + c.get(g)
        });

    // Initial evaluation: b = 4, c = 12, d = 16
    assert_eq!(d.get(&runtime.reactive_graph), 16);
    assert_eq!(d_evals.get(), 1);

    // Update root of diamond: a = 5
    a.set(&runtime, 5);
    // b = 10, c = 15, d = 25
    assert_eq!(d.get(&runtime.reactive_graph), 25);
    assert_eq!(
        d_evals.get(),
        2,
        "Diamond graph must evaluate target at most once"
    );
    println!("OK");

    // -------------------------------------------------------------------------
    // 4. Min-Heap Depth-Ordered Scheduler (Parent + Child Coalescing)
    // -------------------------------------------------------------------------
    print!("[4/6] Verifying depth-ordered dirty scheduler coalesces parent + child updates... ");
    let mut runtime2 = Runtime::new();
    let shared_sig = runtime2.reactive_graph.create_signal(10);
    let p_renders = Rc::new(Cell::new(0));
    let c_renders = Rc::new(Cell::new(0));

    let _parent_id = runtime2.mount_root(ComponentDescriptor::of::<MilestoneParent>(
        ParentChildProps {
            sig: shared_sig,
            parent_renders: p_renders.clone(),
            child_renders: c_renders.clone(),
        },
        NodeKey::Explicit(Key::from("m2_parent")),
    ));

    assert_eq!(p_renders.get(), 1);
    assert_eq!(c_renders.get(), 1);

    // Simultaneous invalidation of Parent (depth 0) and Child (depth 1)
    runtime2.batch(|cx| {
        shared_sig.set(cx, 99);
    });

    // Parent must reconcile first; Child renders exactly ONCE
    assert_eq!(p_renders.get(), 2);
    assert_eq!(
        c_renders.get(),
        2,
        "Child must render only once when parent + child simultaneously invalidated"
    );
    println!("OK");

    // -------------------------------------------------------------------------
    // 5. Unmounted Descendant Pruning & Render Elimination
    // -------------------------------------------------------------------------
    print!("[5/6] Verifying unmounted dirty descendants are cleanly skipped... ");
    let mut runtime3 = Runtime::new();
    let show_sig = runtime3.reactive_graph.create_signal(true);
    let data_sig = runtime3.reactive_graph.create_signal(1);
    let cond_p_renders = Rc::new(Cell::new(0));
    let cond_c_renders = Rc::new(Cell::new(0));

    struct CondParent;
    #[derive(Clone, PartialEq, Debug)]
    struct CPProps {
        show: Signal<bool>,
        data: Signal<i32>,
        p_count: Rc<Cell<usize>>,
        c_count: Rc<Cell<usize>>,
    }
    impl Props for CPProps {}

    impl Component for CondParent {
        type Props = CPProps;
        type Message = ();
        fn create(_: &Self::Props, _: &mut ComponentContext<Self>) -> Self {
            Self
        }
        fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
            props.p_count.set(props.p_count.get() + 1);
            if props.show.get(ctx) {
                UiNode::Component(ComponentDescriptor::of::<LeafComponent>(
                    LeafProps {
                        sig: props.data,
                        render_count: props.c_count.clone(),
                    },
                    NodeKey::Explicit(Key::from("cond_leaf")),
                ))
            } else {
                UiNode::Empty
            }
        }
    }

    let root = runtime3.mount_root(ComponentDescriptor::of::<CondParent>(
        CPProps {
            show: show_sig,
            data: data_sig,
            p_count: cond_p_renders.clone(),
            c_count: cond_c_renders.clone(),
        },
        NodeKey::Explicit(Key::from("cond_root")),
    ));

    assert_eq!(cond_p_renders.get(), 1);
    assert_eq!(cond_c_renders.get(), 1);
    let child_node_id = runtime3.arena.get(root).expect("root exists").children[0];
    assert!(runtime3.arena.contains(child_node_id));

    // Invalidate show (causing unmount) and data (marking child dirty)
    runtime3.batch(|cx| {
        show_sig.set(cx, false);
        data_sig.set(cx, 9999);
    });

    assert_eq!(cond_p_renders.get(), 2);
    assert_eq!(
        cond_c_renders.get(),
        1,
        "Unmounted child must never be re-rendered"
    );
    assert!(!runtime3.arena.contains(child_node_id));
    println!("OK");

    // -------------------------------------------------------------------------
    // 6. Multi-Mutation Batch Coalescing
    // -------------------------------------------------------------------------
    print!("[6/6] Verifying multi-mutation batch coalescing into single flush pass... ");
    let mut runtime4 = Runtime::new();
    let s1 = runtime4.reactive_graph.create_signal(0);
    let s2 = runtime4.reactive_graph.create_signal(10);
    let s3 = runtime4.reactive_graph.create_signal(20);

    let multi_renders = Rc::new(Cell::new(0));

    struct MultiComp;
    #[derive(Clone, PartialEq, Debug)]
    struct MultiProps {
        s1: Signal<i32>,
        s2: Signal<i32>,
        s3: Signal<i32>,
        count: Rc<Cell<usize>>,
    }
    impl Props for MultiProps {}

    impl Component for MultiComp {
        type Props = MultiProps;
        type Message = ();
        fn create(_: &Self::Props, _: &mut ComponentContext<Self>) -> Self {
            Self
        }
        fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
            props.count.set(props.count.get() + 1);
            let total = props.s1.get(ctx) + props.s2.get(ctx) + props.s3.get(ctx);
            UiNode::Text(nyrui_core::node::TextNode::new(format!("Total: {total}")))
        }
    }

    let _m_id = runtime4.mount_root(ComponentDescriptor::of::<MultiComp>(
        MultiProps {
            s1,
            s2,
            s3,
            count: multi_renders.clone(),
        },
        NodeKey::Explicit(Key::from("multi")),
    ));

    assert_eq!(multi_renders.get(), 1);

    // 10 distinct mutations within a single batch
    runtime4.batch(|cx| {
        for i in 1..=10 {
            s1.set(cx, i);
            s2.set(cx, i * 2);
            s3.set(cx, i * 3);
        }
    });

    // Despite 30 signal sets, the component must render only ONCE during flush
    assert_eq!(
        multi_renders.get(),
        2,
        "Batch pass must coalesce all mutations into a single re-render"
    );
    println!("OK");

    println!("\nAll Milestone 2 verification scenarios passed successfully!");
}
