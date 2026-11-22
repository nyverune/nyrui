use nyrui_core::id::NodeId;
use nyrui_core::reactivity::{
    Computed, ComputedState, ObserverId, ReactiveGraph, Signal, SignalError,
};

use std::num::NonZeroU32;

fn test_node_id(index: u32) -> NodeId {
    NodeId::new(index, NonZeroU32::new(1).expect("non-zero"))
}

#[test]
fn signal_get_clones_value() {
    let graph = ReactiveGraph::new();
    let sig = Signal::new(&graph, String::from("initial text"));

    // Reads via .get(&graph)
    let val: String = sig.get(&graph);
    assert_eq!(val, "initial text");

    // Signal retains value after get
    assert_eq!(sig.get(&graph), "initial text");
}

#[test]
fn signal_with_borrows_value() {
    let graph = ReactiveGraph::new();
    let sig = Signal::new(&graph, vec![1, 2, 3, 4, 5]);

    // Inspect without cloning
    let len = sig.with(&graph, |vec| vec.len());
    assert_eq!(len, 5);

    let sum: i32 = sig.with(&graph, |vec| vec.iter().sum());
    assert_eq!(sum, 15);
}

#[test]
fn signal_set_unconditionally_updates_and_dirties() {
    let graph = ReactiveGraph::new();
    let sig = Signal::new(&graph, 10);

    let node = test_node_id(1);

    // Track dependency
    graph.with_observer(ObserverId::Node(node), || {
        assert_eq!(sig.get(&graph), 10);
    });

    assert!(!graph.has_dirty_nodes());

    // set() updates value and marks subscriber dirty
    sig.set(&graph, 20);
    assert_eq!(sig.get(&graph), 20);
    assert!(graph.has_dirty_nodes());
    assert_eq!(graph.take_dirty_nodes(), vec![node]);

    // Setting same value with set() still dirties
    graph.with_observer(ObserverId::Node(node), || {
        let _ = sig.get(&graph);
    });
    assert!(!graph.has_dirty_nodes());

    sig.set(&graph, 20);
    assert!(graph.has_dirty_nodes());
    assert_eq!(graph.take_dirty_nodes(), vec![node]);
}

#[test]
fn signal_update_mutates_in_place_and_dirties() {
    let graph = ReactiveGraph::new();
    let sig = Signal::new(&graph, vec!["alpha".to_string()]);

    let node = test_node_id(2);
    graph.with_observer(ObserverId::Node(node), || {
        sig.with(&graph, |_| ());
    });

    assert!(!graph.has_dirty_nodes());

    // Mutate in place
    let count = sig.update(&graph, |list| {
        list.push("beta".to_string());
        list.len()
    });

    assert_eq!(count, 2);
    assert_eq!(sig.get(&graph), vec!["alpha", "beta"]);
    assert!(graph.has_dirty_nodes());
    assert_eq!(graph.take_dirty_nodes(), vec![node]);
}

#[test]
fn signal_partial_eq_deduplication() {
    let graph = ReactiveGraph::new();
    let sig = Signal::new(&graph, 100);

    let node = test_node_id(3);
    graph.with_observer(ObserverId::Node(node), || {
        let _ = sig.get(&graph);
    });

    assert!(!graph.has_dirty_nodes());

    // 1. set_if_changed with identical value does NOT dirty subscribers
    let changed = sig.set_if_changed(&graph, 100);
    assert!(!changed);
    assert!(!graph.has_dirty_nodes());
    assert_eq!(sig.get(&graph), 100);

    // 2. set_if_changed with different value DOES dirty subscribers
    let changed = sig.set_if_changed(&graph, 200);
    assert!(changed);
    assert!(graph.has_dirty_nodes());
    assert_eq!(graph.take_dirty_nodes(), vec![node]);
    assert_eq!(sig.get(&graph), 200);

    // 3. Re-subscribe and verify subsequent identical write is deduplicated
    graph.with_observer(ObserverId::Node(node), || {
        let _ = sig.get(&graph);
    });
    assert!(!graph.has_dirty_nodes());

    assert!(!sig.set_if_changed(&graph, 200));
    assert!(!graph.has_dirty_nodes());
}

#[test]
fn signal_replace() {
    let graph = ReactiveGraph::new();
    let sig = Signal::new(&graph, 42);

    let old = sig.replace(&graph, 84);
    assert_eq!(old, 42);
    assert_eq!(sig.get(&graph), 84);
}

#[test]
fn generational_slot_reuse_and_aba_protection() {
    let graph = ReactiveGraph::new();
    let sig1 = Signal::new(&graph, "first");
    let id1 = sig1.id();

    assert!(graph.contains(id1));
    assert_eq!(sig1.get(&graph), "first");

    // Dispose sig1
    assert!(sig1.dispose(&graph));
    assert!(!graph.contains(id1));

    // Stale reads fail
    assert_eq!(sig1.try_get(&graph), Err(SignalError::SignalNotFound(id1)));

    // Allocate sig2 into the recycled slot
    let sig2 = Signal::new(&graph, "second");
    let id2 = sig2.id();

    assert_eq!(id1.index, id2.index);
    assert_ne!(id1.generation, id2.generation);

    // sig2 is valid
    assert_eq!(sig2.get(&graph), "second");

    // sig1 remains stale and cannot read sig2's data
    assert_eq!(sig1.try_get(&graph), Err(SignalError::SignalNotFound(id1)));
    assert!(!sig1.dispose(&graph));
}

#[test]
fn type_mismatch_detection() {
    let graph = ReactiveGraph::new();
    let sig_int = Signal::new(&graph, 12345i32);

    // Forge a typed handle with the same SignalId but wrong inner type
    let forged_sig_str: Signal<String> = Signal::from_id(sig_int.id());

    match forged_sig_str.try_get(&graph) {
        Err(SignalError::TypeMismatch { expected, found }) => {
            assert!(expected.contains("i32"));
            assert!(found.contains("String"));
        }
        other => panic!("expected TypeMismatch, got {other:?}"),
    }
}

#[test]
fn clear_node_subscriptions_prevents_stale_notifications() {
    let graph = ReactiveGraph::new();
    let sig_a = Signal::new(&graph, 1);
    let sig_b = Signal::new(&graph, 2);

    let node1 = test_node_id(10);
    let node2 = test_node_id(20);

    // Node 1 observes both signals
    graph.with_observer(ObserverId::Node(node1), || {
        let _ = sig_a.get(&graph);
        let _ = sig_b.get(&graph);
    });

    // Node 2 observes signal B
    graph.with_observer(ObserverId::Node(node2), || {
        let _ = sig_b.get(&graph);
    });

    assert_eq!(graph.subscriber_count(sig_a.id()), 1);
    assert_eq!(graph.subscriber_count(sig_b.id()), 2);

    // Clear subscriptions for node 1 (e.g., node 1 re-renders or unmounts)
    graph.clear_node_subscriptions(node1);
    assert_eq!(graph.subscriber_count(sig_a.id()), 0);
    assert_eq!(graph.subscriber_count(sig_b.id()), 1);

    // Mutating signal A does not dirty anything
    sig_a.set(&graph, 10);
    assert!(!graph.has_dirty_nodes());

    // Mutating signal B dirties only node 2
    sig_b.set(&graph, 20);
    assert_eq!(graph.take_dirty_nodes(), vec![node2]);
}

#[test]
fn signal_works_with_non_clone_non_partialeq_types() {
    struct CustomResource {
        pub payload: Vec<u8>,
    }

    let graph = ReactiveGraph::new();
    let sig: Signal<CustomResource> = Signal::new(
        &graph,
        CustomResource {
            payload: vec![0xCA, 0xFE],
        },
    );

    // Signal itself is Copy and Clone
    let sig_copy = sig;
    assert_eq!(sig, sig_copy);

    let len = sig.with(&graph, |res| res.payload.len());
    assert_eq!(len, 2);

    sig.update(&graph, |res| {
        res.payload.push(0xBA);
        res.payload.push(0xBE);
    });

    sig_copy.with(&graph, |res| {
        assert_eq!(res.payload, vec![0xCA, 0xFE, 0xBA, 0xBE]);
    });
}

#[test]
fn component_context_and_render_context_as_reactive_context() {
    use nyrui_core::component::{ComponentContext, RenderContext};
    use std::cell::Cell;

    let graph = ReactiveGraph::new();
    let sig = Signal::new(&graph, 42);

    let flag = Cell::new(false);
    let node_id = test_node_id(99);

    // 1. ComponentContext
    let comp_ctx = ComponentContext::<()>::new(node_id, &flag).with_reactive_graph(&graph);
    assert_eq!(sig.get(&comp_ctx), 42);
    sig.set(&comp_ctx, 50);
    assert_eq!(sig.get(&comp_ctx), 50);

    // 2. RenderContext
    let render_ctx = RenderContext::<()>::new(node_id, &flag).with_reactive_graph(&graph);
    assert_eq!(sig.get(&render_ctx), 50);
    assert_eq!(sig.with(&render_ctx, |v| *v * 2), 100);
}

#[derive(Clone, Debug, PartialEq)]
struct ReaderProps {
    sig: Signal<i32>,
}

impl nyrui_core::props::Props for ReaderProps {}

struct ReaderComponent {
    sig: Signal<i32>,
}

impl nyrui_core::component::Component for ReaderComponent {
    type Props = ReaderProps;
    type Message = ();

    fn create(
        props: &Self::Props,
        _ctx: &mut nyrui_core::component::ComponentContext<Self>,
    ) -> Self {
        Self { sig: props.sig }
    }

    fn render(
        &self,
        _props: &Self::Props,
        ctx: &nyrui_core::component::RenderContext<Self>,
    ) -> nyrui_core::node::UiNode {
        let val = self.sig.get(ctx);
        nyrui_core::node::UiNode::text(format!("Val: {val}"))
    }
}

#[derive(Clone, Debug, PartialEq)]
struct BranchProps {
    cond: Signal<bool>,
    sig_a: Signal<i32>,
    sig_b: Signal<i32>,
}

impl nyrui_core::props::Props for BranchProps {}

struct BranchComponent {
    cond: Signal<bool>,
    sig_a: Signal<i32>,
    sig_b: Signal<i32>,
}

impl nyrui_core::component::Component for BranchComponent {
    type Props = BranchProps;
    type Message = ();

    fn create(
        props: &Self::Props,
        _ctx: &mut nyrui_core::component::ComponentContext<Self>,
    ) -> Self {
        Self {
            cond: props.cond,
            sig_a: props.sig_a,
            sig_b: props.sig_b,
        }
    }

    fn render(
        &self,
        _props: &Self::Props,
        ctx: &nyrui_core::component::RenderContext<Self>,
    ) -> nyrui_core::node::UiNode {
        if self.cond.get(ctx) {
            let a = self.sig_a.get(ctx);
            nyrui_core::node::UiNode::text(format!("A: {a}"))
        } else {
            let b = self.sig_b.get(ctx);
            nyrui_core::node::UiNode::text(format!("B: {b}"))
        }
    }
}

#[test]
fn component_render_tracks_signal_and_signal_set_dirties_only_reader() {
    use nyrui_core::id::NodeKey;
    use nyrui_core::node::ComponentDescriptor;
    use nyrui_core::runtime::arena::Arena;
    use nyrui_core::runtime::mount_node;
    use std::cell::Cell;
    use std::rc::Rc;

    let graph = Rc::new(ReactiveGraph::new());
    let mut arena = Arena::new().with_reactive_graph(graph.clone());
    let dirty_cell = Cell::new(false);

    let sig_a = Signal::new(&*graph, 10);
    let sig_b = Signal::new(&*graph, 20);

    // Mount Component A reading sig_a
    let desc_a = ComponentDescriptor::of::<ReaderComponent>(
        ReaderProps { sig: sig_a },
        NodeKey::positional(0, 0),
    );
    let id_a = mount_node(&mut arena, None, 0, desc_a, &dirty_cell);

    // Mount Component B reading sig_b
    let desc_b = ComponentDescriptor::of::<ReaderComponent>(
        ReaderProps { sig: sig_b },
        NodeKey::positional(0, 1),
    );
    let id_b = mount_node(&mut arena, None, 0, desc_b, &dirty_cell);

    // Verify subscriptions in both the reactive graph and the arena InstanceNodes
    assert_eq!(graph.subscriber_count(sig_a.id()), 1);
    assert_eq!(graph.subscriber_count(sig_b.id()), 1);
    assert!(
        graph
            .subscribers(sig_a.id())
            .expect("subscribers")
            .contains(&ObserverId::Node(id_a))
    );
    assert!(
        !graph
            .subscribers(sig_a.id())
            .expect("subscribers")
            .contains(&ObserverId::Node(id_b))
    );
    assert!(
        graph
            .subscribers(sig_b.id())
            .expect("subscribers")
            .contains(&ObserverId::Node(id_b))
    );

    assert_eq!(
        arena.get(id_a).expect("node a").subscriptions.as_slice(),
        &[sig_a.id()]
    );
    assert_eq!(
        arena.get(id_b).expect("node b").subscriptions.as_slice(),
        &[sig_b.id()]
    );

    assert!(!graph.has_dirty_nodes());

    // Mutating sig_a dirties ONLY Component A!
    sig_a.set(&*graph, 99);
    assert!(graph.is_node_dirty(id_a));
    assert!(!graph.is_node_dirty(id_b));
    let dirty = graph.take_dirty_nodes();
    assert_eq!(dirty, vec![id_a]);

    // Mutating sig_b dirties ONLY Component B!
    sig_b.set(&*graph, 100);
    assert!(graph.is_node_dirty(id_b));
    assert!(!graph.is_node_dirty(id_a));
    let dirty = graph.take_dirty_nodes();
    assert_eq!(dirty, vec![id_b]);
}

#[test]
fn unmount_subtree_clears_signal_subscriptions() {
    use nyrui_core::id::NodeKey;
    use nyrui_core::node::ComponentDescriptor;
    use nyrui_core::runtime::arena::Arena;
    use nyrui_core::runtime::{mount_node, unmount_subtree};
    use std::cell::Cell;
    use std::rc::Rc;

    let graph = Rc::new(ReactiveGraph::new());
    let mut arena = Arena::new().with_reactive_graph(graph.clone());
    let dirty_cell = Cell::new(false);

    let sig = Signal::new(&*graph, 42);

    let desc =
        ComponentDescriptor::of::<ReaderComponent>(ReaderProps { sig }, NodeKey::positional(0, 0));
    let id = mount_node(&mut arena, None, 0, desc, &dirty_cell);

    assert_eq!(graph.subscriber_count(sig.id()), 1);
    assert!(
        graph
            .subscribers(sig.id())
            .expect("subscribers")
            .contains(&ObserverId::Node(id))
    );
    assert_eq!(
        arena.get(id).expect("node").subscriptions.as_slice(),
        &[sig.id()]
    );

    // Unmount the component subtree
    unmount_subtree(&mut arena, id, &dirty_cell);

    // Component is unmounted from arena
    assert!(!arena.contains(id));

    // The signal's subscriber list must now be empty!
    assert_eq!(graph.subscriber_count(sig.id()), 0);

    // Mutating the signal does NOT mark the unmounted component dirty
    sig.set(&*graph, 100);
    assert!(!graph.has_dirty_nodes());
}

#[test]
fn re_rendering_node_clears_inactive_conditional_branch_subscriptions() {
    use nyrui_core::id::NodeKey;
    use nyrui_core::node::ComponentDescriptor;
    use nyrui_core::runtime::arena::Arena;
    use nyrui_core::runtime::mount_node;
    use std::cell::Cell;
    use std::rc::Rc;

    let graph = Rc::new(ReactiveGraph::new());
    let mut arena = Arena::new().with_reactive_graph(graph.clone());
    let dirty_cell = Cell::new(false);

    let cond = Signal::new(&*graph, true);
    let sig_a = Signal::new(&*graph, 1);
    let sig_b = Signal::new(&*graph, 2);

    let desc = ComponentDescriptor::of::<BranchComponent>(
        BranchProps { cond, sig_a, sig_b },
        NodeKey::positional(0, 0),
    );
    let id = mount_node(&mut arena, None, 0, desc, &dirty_cell);

    // Initial render (cond is true): component observed cond and sig_a
    assert_eq!(graph.subscriber_count(cond.id()), 1);
    assert_eq!(graph.subscriber_count(sig_a.id()), 1);
    assert_eq!(graph.subscriber_count(sig_b.id()), 0);
    assert!(
        arena
            .get(id)
            .expect("node")
            .subscriptions
            .contains(&cond.id())
    );
    assert!(
        arena
            .get(id)
            .expect("node")
            .subscriptions
            .contains(&sig_a.id())
    );
    assert!(
        !arena
            .get(id)
            .expect("node")
            .subscriptions
            .contains(&sig_b.id())
    );

    // Switch branch: cond = false
    cond.set(&*graph, false);
    assert_eq!(graph.take_dirty_nodes(), vec![id]);

    // Re-render the component
    let _ = arena.render(id, &dirty_cell).expect("render succeeds");

    // After re-rendering with cond = false:
    // sig_a's subscription must be cleared!
    // sig_b's subscription must now be active!
    assert_eq!(graph.subscriber_count(cond.id()), 1);
    assert_eq!(graph.subscriber_count(sig_a.id()), 0);
    assert_eq!(graph.subscriber_count(sig_b.id()), 1);
    assert!(
        arena
            .get(id)
            .expect("node")
            .subscriptions
            .contains(&cond.id())
    );
    assert!(
        !arena
            .get(id)
            .expect("node")
            .subscriptions
            .contains(&sig_a.id())
    );
    assert!(
        arena
            .get(id)
            .expect("node")
            .subscriptions
            .contains(&sig_b.id())
    );

    // Mutating inactive branch (sig_a) does NOT dirty the component!
    sig_a.set(&*graph, 999);
    assert!(!graph.has_dirty_nodes());

    // Mutating active branch (sig_b) DOES dirty the component!
    sig_b.set(&*graph, 888);
    assert!(graph.has_dirty_nodes());
    assert_eq!(graph.take_dirty_nodes(), vec![id]);
}

#[test]
fn diamond_dependency_single_evaluation() {
    use std::cell::Cell;
    use std::rc::Rc;

    // Diamond dependency graph:
    //         A (Signal)
    //        / \
    //       B   C (Computed)
    //        \ /
    //         D (Computed)
    let graph = Rc::new(ReactiveGraph::new());
    let a = Signal::new(&*graph, 1);

    let b_evals = Rc::new(Cell::new(0));
    let c_evals = Rc::new(Cell::new(0));
    let d_evals = Rc::new(Cell::new(0));

    let b = {
        let b_evals = b_evals.clone();
        Computed::with_context(&*graph, move |cx| {
            b_evals.set(b_evals.get() + 1);
            a.get(cx) * 10
        })
    };

    let c = {
        let c_evals = c_evals.clone();
        Computed::with_context(&*graph, move |cx| {
            c_evals.set(c_evals.get() + 1);
            a.get(cx) + 5
        })
    };

    let d = {
        let d_evals = d_evals.clone();
        Computed::with_context(&*graph, move |cx| {
            d_evals.set(d_evals.get() + 1);
            b.get(cx) + c.get(cx)
        })
    };

    // 1. Lazy evaluation: closures are NOT run upon creation
    assert_eq!(b_evals.get(), 0);
    assert_eq!(c_evals.get(), 0);
    assert_eq!(d_evals.get(), 0);

    // 2. Initial read of D:
    // a = 1 -> b = 10, c = 6 -> d = 16
    assert_eq!(d.get(&*graph), 16);
    assert_eq!(b_evals.get(), 1);
    assert_eq!(c_evals.get(), 1);
    assert_eq!(d_evals.get(), 1);

    // 3. Subsequent reads of D in the same batch return cached value without recomputing
    assert_eq!(d.get(&*graph), 16);
    assert_eq!(d.get(&*graph), 16);
    assert_eq!(d_evals.get(), 1);

    // 4. Update A:
    a.set(&*graph, 2);

    // Push phase: A eagerly marked B, C, and D dirty, but evaluation is lazy!
    assert_eq!(graph.computed_state(b.id()), Some(ComputedState::Dirty));
    assert_eq!(graph.computed_state(c.id()), Some(ComputedState::Dirty));
    assert_eq!(graph.computed_state(d.id()), Some(ComputedState::Dirty));
    assert_eq!(b_evals.get(), 1);
    assert_eq!(c_evals.get(), 1);
    assert_eq!(d_evals.get(), 1);

    // 5. Read D:
    // a = 2 -> b = 20, c = 7 -> d = 27
    assert_eq!(d.get(&*graph), 27);

    // In a diamond dependency graph, D must recompute AT MOST ONCE per batch!
    assert_eq!(b_evals.get(), 2);
    assert_eq!(c_evals.get(), 2);
    assert_eq!(d_evals.get(), 2);

    // 6. Multiple subsequent reads still return the clean cached value
    assert_eq!(d.get(&*graph), 27);
    assert_eq!(b.get(&*graph), 20);
    assert_eq!(c.get(&*graph), 7);
    assert_eq!(b_evals.get(), 2);
    assert_eq!(c_evals.get(), 2);
    assert_eq!(d_evals.get(), 2);
}

#[test]
#[should_panic(expected = "Dependency cycle detected in reactive graph")]
fn computed_cycle_detection_panics_self_cycle() {
    use std::cell::Cell;
    use std::rc::Rc;

    let graph = Rc::new(ReactiveGraph::new());
    let comp_cell: Rc<Cell<Option<Computed<i32>>>> = Rc::new(Cell::new(None));

    let comp = {
        let comp_cell = comp_cell.clone();
        Computed::with_context(&*graph, move |cx| {
            if let Some(c) = comp_cell.get() {
                c.get(cx) + 1
            } else {
                0
            }
        })
    };
    comp_cell.set(Some(comp));

    // Reading comp will cause it to recursively read itself, triggering the cycle panic!
    let _ = comp.get(&*graph);
}

#[test]
#[should_panic(expected = "Dependency cycle detected in reactive graph")]
fn computed_cycle_detection_panics_mutual_cycle() {
    use std::cell::Cell;
    use std::rc::Rc;

    // Mutual cycle: A -> B -> A
    let graph = Rc::new(ReactiveGraph::new());
    let comp_b_cell: Rc<Cell<Option<Computed<i32>>>> = Rc::new(Cell::new(None));

    let comp_a = {
        let comp_b_cell = comp_b_cell.clone();
        Computed::with_context(&*graph, move |cx| {
            if let Some(b) = comp_b_cell.get() {
                b.get(cx) + 1
            } else {
                1
            }
        })
    };

    let comp_b = Computed::with_context(&*graph, move |cx| comp_a.get(cx) + 10);
    comp_b_cell.set(Some(comp_b));

    // Evaluating comp_a reads comp_b which reads comp_a -> panics!
    let _ = comp_a.get(&*graph);
}

#[test]
fn computed_dynamic_conditional_dependency_pruning() {
    use std::cell::Cell;
    use std::rc::Rc;

    let graph = Rc::new(ReactiveGraph::new());
    let cond = Signal::new(&*graph, true);
    let sig_a = Signal::new(&*graph, 10);
    let sig_b = Signal::new(&*graph, 20);

    let eval_count = Rc::new(Cell::new(0));

    let comp = {
        let eval_count = eval_count.clone();
        Computed::with_context(&*graph, move |cx| {
            eval_count.set(eval_count.get() + 1);
            if cond.get(cx) {
                sig_a.get(cx)
            } else {
                sig_b.get(cx)
            }
        })
    };

    // First read (cond == true): reads cond and sig_a
    assert_eq!(comp.get(&*graph), 10);
    assert_eq!(eval_count.get(), 1);
    assert_eq!(graph.subscriber_count(cond.id()), 1);
    assert_eq!(graph.subscriber_count(sig_a.id()), 1);
    assert_eq!(graph.subscriber_count(sig_b.id()), 0);

    // Switch branch: cond = false
    cond.set(&*graph, false);
    assert_eq!(graph.computed_state(comp.id()), Some(ComputedState::Dirty));

    // Re-read comp: now reads cond and sig_b
    assert_eq!(comp.get(&*graph), 20);
    assert_eq!(eval_count.get(), 2);
    // sig_a must have been pruned from dependencies!
    assert_eq!(graph.subscriber_count(cond.id()), 1);
    assert_eq!(graph.subscriber_count(sig_a.id()), 0);
    assert_eq!(graph.subscriber_count(sig_b.id()), 1);

    // Mutating inactive branch (sig_a) does NOT mark computed dirty!
    sig_a.set(&*graph, 999);
    assert_eq!(graph.computed_state(comp.id()), Some(ComputedState::Clean));
    assert_eq!(comp.get(&*graph), 20);
    assert_eq!(eval_count.get(), 2); // 0 additional evaluations!

    // Mutating active branch (sig_b) DOES mark computed dirty!
    sig_b.set(&*graph, 30);
    assert_eq!(graph.computed_state(comp.id()), Some(ComputedState::Dirty));
    assert_eq!(comp.get(&*graph), 30);
    assert_eq!(eval_count.get(), 3);
}

#[derive(Clone, Debug, PartialEq)]
struct CompViewerProps {
    comp: Computed<i32>,
}

impl nyrui_core::props::Props for CompViewerProps {}

struct CompViewerComponent {
    comp: Computed<i32>,
}

impl nyrui_core::component::Component for CompViewerComponent {
    type Props = CompViewerProps;
    type Message = ();

    fn create(
        props: &Self::Props,
        _ctx: &mut nyrui_core::component::ComponentContext<Self>,
    ) -> Self {
        Self { comp: props.comp }
    }

    fn render(
        &self,
        _props: &Self::Props,
        ctx: &nyrui_core::component::RenderContext<Self>,
    ) -> nyrui_core::node::UiNode {
        let val = self.comp.get(ctx);
        nyrui_core::node::UiNode::text(format!("Value: {val}"))
    }
}

#[test]
fn computed_eagerly_schedules_downstream_component() {
    use nyrui_core::id::NodeKey;
    use nyrui_core::node::ComponentDescriptor;
    use nyrui_core::runtime::arena::Arena;
    use nyrui_core::runtime::mount_node;
    use std::cell::Cell;
    use std::rc::Rc;

    let graph = Rc::new(ReactiveGraph::new());
    let mut arena = Arena::new().with_reactive_graph(graph.clone());
    let dirty_cell = Cell::new(false);

    let sig = Signal::new(&*graph, 5);
    let comp = Computed::with_context(&*graph, move |cx| sig.get(cx) * 10);

    let desc = ComponentDescriptor::of::<CompViewerComponent>(
        CompViewerProps { comp },
        NodeKey::positional(0, 0),
    );
    let id = mount_node(&mut arena, None, 0, desc, &dirty_cell);

    // Initial render: component read comp (val = 50)
    assert_eq!(comp.get(&*graph), 50);
    assert!(!graph.has_dirty_nodes());

    // Mutating sig eagerly marks downstream component dirty!
    sig.set(&*graph, 6);
    assert!(graph.is_node_dirty(id));
    assert_eq!(graph.take_dirty_nodes(), vec![id]);

    // Component re-renders and pulls the new computed value
    let _ = arena.render(id, &dirty_cell).expect("render succeeds");
    assert_eq!(comp.get(&*graph), 60);
}

#[test]
fn computed_disposal_and_generational_lookup() {
    let graph = std::rc::Rc::new(ReactiveGraph::new());
    let sig = Signal::new(&*graph, 100);
    let comp1 = {
        let g = graph.clone();
        Computed::new(&*graph, move || sig.get(&*g) + 1)
    };

    assert_eq!(graph.computed_len(), 1);
    assert!(graph.contains_computed(comp1.id()));
    assert_eq!(comp1.get(&*graph), 101);

    // Dispose comp1
    assert!(graph.dispose_computed(comp1.id()));
    assert_eq!(graph.computed_len(), 0);
    assert!(!graph.contains_computed(comp1.id()));

    // Stale handle fails lookup
    assert_eq!(
        comp1.try_get(&*graph),
        Err(SignalError::ComputedNotFound(comp1.id()))
    );

    // Allocate comp2 reusing slot with new generation
    let comp2 = {
        let g = graph.clone();
        Computed::new(&*graph, move || sig.get(&*g) + 2)
    };
    assert_eq!(graph.computed_len(), 1);
    assert_eq!(comp2.id().index, comp1.id().index);
    assert_ne!(comp2.id().generation, comp1.id().generation);
    assert_eq!(comp2.get(&*graph), 102);

    // comp1 remains invalid
    assert!(!graph.contains_computed(comp1.id()));
    assert!(graph.contains_computed(comp2.id()));
}
