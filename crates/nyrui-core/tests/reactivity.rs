use nyrui_core::id::NodeId;
use nyrui_core::reactivity::{ObserverId, ReactiveGraph, Signal, SignalError};

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
