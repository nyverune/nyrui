//! Integration tests for the cross-thread async message channel, `UiSender`, and `ctx.spawn`.
//!
//! Validates the architectural guarantees specified in **RFC-0001 §3.1 and §13.1**:
//! - Cross-thread message boundary between background Tokio tasks and the UI thread.
//! - Scoped Tokio task spawning via `ctx.spawn` tied to a component's [`NodeId`].
//! - Immediate task cancellation via [`tokio::task::AbortHandle`] upon component unmount.
//! - Silent discarding of in-flight messages targeting unmounted components (generational safety).

use nyrui_core::component::{Component, ComponentContext, RenderContext, ShouldRender};
use nyrui_core::id::{Key, NodeKey};
use nyrui_core::node::component::ComponentDescriptor;
use nyrui_core::node::{TextNode, UiNode};
use nyrui_core::props::Props;
use nyrui_core::reactivity::Signal;
use nyrui_core::runtime::Runtime;
use nyrui_core::runtime::channel::{UiSendError, UiSender};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

// -----------------------------------------------------------------------------
// Component Definitions for Async Testing
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum AsyncCounterMsg {
    Add(i32),
    Set(i32),
}

struct AsyncCounterComponent {
    count: i32,
}

impl Component for AsyncCounterComponent {
    type Props = ();
    type Message = AsyncCounterMsg;

    fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self { count: 0 }
    }

    fn update(&mut self, msg: Self::Message, ctx: &mut ComponentContext<Self>) -> ShouldRender {
        match msg {
            AsyncCounterMsg::Add(n) => {
                self.count += n;
                ctx.invalidate();
                ShouldRender::Yes
            }
            AsyncCounterMsg::Set(n) => {
                self.count = n;
                ctx.invalidate();
                ShouldRender::Yes
            }
        }
    }

    fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        UiNode::Text(TextNode::new(format!("Count: {}", self.count)))
    }
}

// -----------------------------------------------------------------------------
// Core Acceptance Criterion 1: Background Tokio task updates UI component state
// -----------------------------------------------------------------------------

#[tokio::test]
async fn background_tokio_task_updates_component_state_on_ui_thread() {
    let mut runtime = Runtime::new();

    struct AutoFetchComponent {
        data: String,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum FetchMsg {
        Loaded(String),
    }

    impl Component for AutoFetchComponent {
        type Props = ();
        type Message = FetchMsg;

        fn create(_props: &Self::Props, ctx: &mut ComponentContext<Self>) -> Self {
            // Spawns a Tokio task using ctx.spawn that returns FetchMsg directly
            ctx.spawn(async move {
                tokio::time::sleep(Duration::from_millis(15)).await;
                FetchMsg::Loaded("FetchedPayload".to_string())
            });

            Self {
                data: "Initial".to_string(),
            }
        }

        fn update(&mut self, msg: Self::Message, ctx: &mut ComponentContext<Self>) -> ShouldRender {
            match msg {
                FetchMsg::Loaded(val) => {
                    self.data = val;
                    ctx.invalidate();
                    ShouldRender::Yes
                }
            }
        }

        fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
            UiNode::Text(TextNode::new(self.data.clone()))
        }
    }

    let root_id = runtime.mount_root(ComponentDescriptor::of::<AutoFetchComponent>(
        (),
        NodeKey::Explicit(Key::from("auto_fetch")),
    ));

    // Verify initial state
    runtime
        .arena
        .with_instance(root_id, |inst, _| {
            let comp = inst
                .as_any()
                .downcast_ref::<AutoFetchComponent>()
                .expect("downcast to AutoFetchComponent");
            assert_eq!(comp.data, "Initial");
        })
        .expect("inspect instance");

    // Wait for the background task to complete and deliver its message via UiSender
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Verify message is pending in runtime's ui_receiver
    assert!(!runtime.ui_receiver.is_empty());
    assert!(runtime.is_dirty());

    // Flush the runtime (drains async messages, updates instance, reconciles in depth order)
    let rendered = runtime.flush();
    assert!(
        rendered >= 1,
        "Component and its children must be re-rendered on update (rendered: {rendered})"
    );

    // Verify updated state on the UI thread
    runtime
        .arena
        .with_instance(root_id, |inst, _| {
            let comp = inst
                .as_any()
                .downcast_ref::<AutoFetchComponent>()
                .expect("downcast to AutoFetchComponent");
            assert_eq!(comp.data, "FetchedPayload");
        })
        .expect("inspect instance");
}

// -----------------------------------------------------------------------------
// Core Acceptance Criterion 2: Task cancellation & ignore after unmount
// -----------------------------------------------------------------------------

#[tokio::test]
async fn task_is_cancelled_and_results_ignored_after_component_unmount() {
    let mut runtime = Runtime::new();

    let task_completed_flag = Arc::new(AtomicBool::new(false));

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum LongTaskMsg {
        Done,
    }

    #[derive(Clone, Debug)]
    struct LongChildProps(Arc<AtomicBool>);
    impl Props for LongChildProps {}

    struct LongRunningChild {
        updated: bool,
    }

    impl Component for LongRunningChild {
        type Props = LongChildProps;
        type Message = LongTaskMsg;

        fn create(props: &Self::Props, ctx: &mut ComponentContext<Self>) -> Self {
            let flag = props.0.clone();
            // Spawns a background task that sleeps for 200ms
            ctx.spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                flag.store(true, Ordering::SeqCst);
                LongTaskMsg::Done
            });

            Self { updated: false }
        }

        fn update(
            &mut self,
            msg: Self::Message,
            _ctx: &mut ComponentContext<Self>,
        ) -> ShouldRender {
            match msg {
                LongTaskMsg::Done => {
                    self.updated = true;
                    ShouldRender::Yes
                }
            }
        }

        fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
            UiNode::Empty
        }
    }

    let show_signal = runtime.reactive_graph.create_signal(true);

    struct DynamicParent;
    #[derive(Clone, Debug)]
    struct ParentProps {
        show: Signal<bool>,
        flag: Arc<AtomicBool>,
    }
    impl Props for ParentProps {}

    impl Component for DynamicParent {
        type Props = ParentProps;
        type Message = ();

        fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
            Self
        }

        fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
            if props.show.get(ctx) {
                UiNode::Component(ComponentDescriptor::of::<LongRunningChild>(
                    LongChildProps(props.flag.clone()),
                    NodeKey::Explicit(Key::from("long_running_child")),
                ))
            } else {
                UiNode::Empty
            }
        }
    }

    let root_id = runtime.mount_root(ComponentDescriptor::of::<DynamicParent>(
        ParentProps {
            show: show_signal,
            flag: task_completed_flag.clone(),
        },
        NodeKey::Explicit(Key::from("parent")),
    ));

    let child_id = runtime.arena.get(root_id).expect("parent exists").children[0];

    // Child is mounted, task is actively tracked in TaskTracker
    assert!(runtime.arena.contains(child_id));
    assert_eq!(runtime.task_tracker.task_count(child_id), 1);
    assert_eq!(runtime.task_tracker.total_tasks(), 1);

    // Unmount child by toggling show_signal in a batch pass
    runtime.batch(|cx| {
        show_signal.set(cx, false);
    });

    // Verify child was unmounted from the arena
    assert!(!runtime.arena.contains(child_id));

    // Verify TaskTracker immediately aborted and cleaned up the task handle
    assert_eq!(runtime.task_tracker.task_count(child_id), 0);
    assert_eq!(runtime.task_tracker.total_tasks(), 0);

    // Wait longer than the task's 200ms duration (300ms)
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The aborted task must NOT have reached completion
    assert!(
        !task_completed_flag.load(Ordering::SeqCst),
        "Aborted background task must not run to completion"
    );

    // Additionally verify generational safety: if a message arrives targeting the unmounted node,
    // drain_messages safely drops it without downcast failure or panic
    let sender = runtime.ui_sender();
    sender
        .send_typed(child_id, LongTaskMsg::Done)
        .expect("send succeeds");
    let processed = runtime.process_async_messages();
    assert_eq!(
        processed, 0,
        "Stale messages targeting unmounted nodes must be cleanly ignored"
    );
}

// -----------------------------------------------------------------------------
// Direct UiSender & UiReceiver Channel Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn direct_ui_sender_and_receiver_cross_thread_dispatch() {
    let mut runtime = Runtime::new();

    let root_id = runtime.mount_root(ComponentDescriptor::of::<AsyncCounterComponent>(
        (),
        NodeKey::Explicit(Key::from("counter")),
    ));

    let sender = runtime.ui_sender();

    // Spawn 5 concurrent tasks sending messages via sender.clone()
    let mut handles = Vec::new();
    for i in 1..=5 {
        let tx = sender.clone();
        handles.push(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(5 * i as u64)).await;
            tx.send_typed(root_id, AsyncCounterMsg::Add(10))
                .expect("send succeeds");
        }));
    }

    for h in handles {
        h.await.expect("join task");
    }

    // Process all 5 delivered messages
    let processed = runtime.process_async_messages();
    assert_eq!(processed, 5);

    // Reconcile
    runtime.flush();

    // Total count must be 50
    runtime
        .arena
        .with_instance(root_id, |inst, _| {
            let comp = inst
                .as_any()
                .downcast_ref::<AsyncCounterComponent>()
                .expect("downcast");
            assert_eq!(comp.count, 50);
        })
        .expect("inspect instance");

    // Also dispatch a Set message directly
    sender
        .send_typed(root_id, AsyncCounterMsg::Set(100))
        .expect("send succeeds");
    let processed = runtime.process_async_messages();
    assert_eq!(processed, 1);
    runtime.flush();
    runtime
        .arena
        .with_instance(root_id, |inst, _| {
            let comp = inst
                .as_any()
                .downcast_ref::<AsyncCounterComponent>()
                .expect("downcast");
            assert_eq!(comp.count, 100);
        })
        .expect("inspect instance");
}

#[tokio::test]
async fn bounded_ui_sender_capacity_and_backpressure() {
    let (tx, mut rx) = UiSender::bounded(2);
    let target = nyrui_core::id::NodeId::new(1, std::num::NonZeroU32::new(1).expect("non-zero"));

    // First 2 sends succeed
    assert_eq!(tx.send_typed(target, 1), Ok(()));
    assert_eq!(tx.send_typed(target, 2), Ok(()));

    // Third non-blocking send fails with ChannelFull
    assert_eq!(tx.send_typed(target, 3), Err(UiSendError::ChannelFull));

    // Drain one message
    let msg1 = rx.recv().await.expect("recv msg");
    assert_eq!(msg1.target, target);

    // Now send_async succeeds
    assert_eq!(tx.send_typed_async(target, 4).await, Ok(()));

    // Drop receiver: send fails with ChannelClosed
    drop(rx);
    assert_eq!(tx.send_typed(target, 5), Err(UiSendError::ChannelClosed));
}

#[tokio::test]
async fn multiple_tasks_scoped_to_same_component_all_cancelled_on_unmount() {
    let mut runtime = Runtime::new();

    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    #[derive(Clone, Debug)]
    struct MultiProps(Arc<std::sync::atomic::AtomicUsize>);
    impl Props for MultiProps {}

    struct MultiTaskChild;
    impl Component for MultiTaskChild {
        type Props = MultiProps;
        type Message = ();

        fn create(props: &Self::Props, ctx: &mut ComponentContext<Self>) -> Self {
            for _ in 0..4 {
                let c = props.0.clone();
                ctx.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    c.fetch_add(1, Ordering::SeqCst);
                });
            }
            Self
        }

        fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
            UiNode::Empty
        }
    }

    let toggle_sig = runtime.reactive_graph.create_signal(true);

    struct Parent;
    #[derive(Clone, Debug)]
    struct PProps {
        toggle: Signal<bool>,
        counter: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl Props for PProps {}

    impl Component for Parent {
        type Props = PProps;
        type Message = ();

        fn create(_: &Self::Props, _: &mut ComponentContext<Self>) -> Self {
            Self
        }

        fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
            if props.toggle.get(ctx) {
                UiNode::Component(ComponentDescriptor::of::<MultiTaskChild>(
                    MultiProps(props.counter.clone()),
                    NodeKey::Explicit(Key::from("multi_child")),
                ))
            } else {
                UiNode::Empty
            }
        }
    }

    let root_id = runtime.mount_root(ComponentDescriptor::of::<Parent>(
        PProps {
            toggle: toggle_sig,
            counter: counter.clone(),
        },
        NodeKey::Explicit(Key::from("root")),
    ));

    let child_id = runtime.arena.get(root_id).expect("parent exists").children[0];

    // All 4 tasks tracked under child_id
    assert_eq!(runtime.task_tracker.task_count(child_id), 4);

    // Unmount child before any task completes
    runtime.batch(|cx| {
        toggle_sig.set(cx, false);
    });

    assert_eq!(runtime.task_tracker.task_count(child_id), 0);

    // Wait 250ms (longer than the 150ms sleep)
    tokio::time::sleep(Duration::from_millis(250)).await;

    // None of the 4 tasks should have incremented the counter
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "All 4 tasks must be aborted"
    );
}

#[tokio::test]
async fn task_tracker_cancel_all_aborts_all_active_tasks() {
    let tracker = nyrui_core::runtime::task::TaskTracker::new();
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();

    let node_id = nyrui_core::id::NodeId::new(42, std::num::NonZeroU32::new(1).expect("non-zero"));

    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        flag_clone.store(true, Ordering::SeqCst);
    });

    tracker.register(node_id, handle.abort_handle());
    assert_eq!(tracker.task_count(node_id), 1);
    assert_eq!(tracker.total_tasks(), 1);

    tracker.cancel_all();
    assert_eq!(tracker.total_tasks(), 0);

    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(!flag.load(Ordering::SeqCst));
}
