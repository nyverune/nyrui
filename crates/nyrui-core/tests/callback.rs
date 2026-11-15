//! Integration tests for Callback<In>, MessageQueue, and child-to-parent communication.

use nyrui_core::callback::Callback;
use nyrui_core::component::{Component, ComponentContext, RenderContext, ShouldRender};
use nyrui_core::id::{Key, NodeKey};
use nyrui_core::node::UiNode;
use nyrui_core::props::Props;
use nyrui_core::runtime::arena::{Arena, InstanceNode, NodeFlags};
use nyrui_core::runtime::message::MessageQueue;

use std::cell::Cell;

// -----------------------------------------------------------------------------
// Parent & Child Component Definitions
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ParentProps;

impl Props for ParentProps {}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ParentMsg {
    Add(i32),
    Reset,
}

#[derive(Debug)]
struct ParentComponent {
    total: i32,
}

impl Component for ParentComponent {
    type Props = ParentProps;
    type Message = ParentMsg;

    fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self { total: 0 }
    }

    fn update(&mut self, msg: Self::Message, _ctx: &mut ComponentContext<Self>) -> ShouldRender {
        match msg {
            ParentMsg::Add(val) => {
                self.total += val;
                ShouldRender::Yes
            }
            ParentMsg::Reset => {
                self.total = 0;
                ShouldRender::Yes
            }
        }
    }

    fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        UiNode::text(format!("Total: {}", self.total))
    }
}

#[derive(Clone, Debug)]
struct ChildButtonProps {
    on_click: Callback<i32>,
}

impl Props for ChildButtonProps {
    fn props_eq(&self, other: &Self) -> bool {
        self.on_click == other.on_click
    }
}

// -----------------------------------------------------------------------------
// Integration Tests
// -----------------------------------------------------------------------------

#[test]
fn child_to_parent_callback_updates_parent_state() {
    let mut arena = Arena::new();
    let queue = MessageQueue::new();
    let dirty_cell = Cell::new(false);

    // 1. Allocate Parent Component with the shared message queue
    let parent_id = arena.allocate(|id| {
        let comp = ParentComponent { total: 0 };
        InstanceNode::new(
            id,
            None,
            0,
            NodeKey::Explicit(Key::from("parent")),
            comp,
            ParentProps,
        )
    });

    // 2. Parent creates a callback bound to its update method
    let ctx =
        ComponentContext::<ParentComponent>::with_queue(parent_id, &dirty_cell, queue.clone());
    let add_callback: Callback<i32> = ctx.callback(ParentMsg::Add);

    // 3. Child button receives the callback in props and emits an event
    let child_props = ChildButtonProps {
        on_click: add_callback.clone(),
    };

    assert_eq!(queue.len(), 0);
    child_props.on_click.emit(15);
    child_props.on_click.emit(25);
    assert_eq!(queue.len(), 2);

    // 4. Process messages in the arena
    let processed = arena.process_message_queue(&queue, &dirty_cell);
    assert_eq!(processed, 2);
    assert_eq!(queue.len(), 0);

    // 5. Verify parent state mutated and node marked dirty
    let parent_node = arena.get(parent_id).expect("parent exists");
    assert!(parent_node.flags.contains(NodeFlags::DIRTY));

    // Render parent to verify updated total
    let vdom = arena
        .render(parent_id, &dirty_cell)
        .expect("render succeeds");
    match vdom {
        UiNode::Text(t) => assert_eq!(t.content.as_str(), "Total: 40"),
        other => panic!("expected text node, got {other:?}"),
    }
}

#[test]
fn callback_targeting_unmounted_node_safely_noops() {
    let mut arena = Arena::new();
    let queue = MessageQueue::new();
    let dirty_cell = Cell::new(false);

    // 1. Allocate Parent Component
    let parent_id = arena.allocate(|id| {
        InstanceNode::new(
            id,
            None,
            0,
            NodeKey::Explicit(Key::from("parent")),
            ParentComponent { total: 100 },
            ParentProps,
        )
    });

    // 2. Create callback targeting parent_id
    let ctx =
        ComponentContext::<ParentComponent>::with_queue(parent_id, &dirty_cell, queue.clone());
    let callback: Callback<()> = ctx.callback(|_| ParentMsg::Reset);

    // 3. Unmount the parent component from the arena
    let removed = arena.remove(parent_id);
    assert!(removed.is_some());
    assert!(!arena.contains(parent_id));

    // 4. Stale callback is emitted by an orphaned or lingering child
    callback.emit(());
    assert_eq!(queue.len(), 1);

    // 5. Draining the queue safely discards the message without panicking
    let processed = arena.process_message_queue(&queue, &dirty_cell);
    assert_eq!(processed, 0);
    assert_eq!(queue.len(), 0);

    // 6. Allocate a new component that reuses the slot index with a new generation
    let new_id = arena.allocate(|id| {
        InstanceNode::new(
            id,
            None,
            0,
            NodeKey::Explicit(Key::from("new_component")),
            ParentComponent { total: 50 },
            ParentProps,
        )
    });
    assert_eq!(new_id.index, parent_id.index);
    assert_ne!(new_id.generation, parent_id.generation);

    // 7. Clear initial DIRTY flag from creation to test that stale callback does not re-dirty it
    let new_node_mut = arena.get_mut(new_id).expect("new node exists");
    new_node_mut.flags.remove(NodeFlags::DIRTY);
    assert!(!new_node_mut.flags.contains(NodeFlags::DIRTY));

    // 8. Emitting with stale callback targeting the old generation does NOT affect the new instance
    callback.emit(());
    let processed2 = arena.process_message_queue(&queue, &dirty_cell);
    assert_eq!(processed2, 0);

    let new_node = arena.get(new_id).expect("new node exists");
    assert!(!new_node.flags.contains(NodeFlags::DIRTY));
}

#[test]
fn callback_combinator_reform() {
    let queue = MessageQueue::new();
    let dirty_cell = Cell::new(false);
    let mut arena = Arena::new();

    let parent_id = arena.allocate(|id| {
        InstanceNode::new(
            id,
            None,
            0,
            NodeKey::Positional { scope: 0, index: 0 },
            ParentComponent { total: 0 },
            ParentProps,
        )
    });

    let ctx =
        ComponentContext::<ParentComponent>::with_queue(parent_id, &dirty_cell, queue.clone());
    let int_callback: Callback<i32> = ctx.callback(ParentMsg::Add);

    // Reform callback from &str to i32 (parsing length)
    let str_callback: Callback<&str> = int_callback.reform(|text: &str| text.len() as i32);
    str_callback.emit("testing");

    let processed = arena.process_message_queue(&queue, &dirty_cell);
    assert_eq!(processed, 1);

    let vdom = arena
        .render(parent_id, &dirty_cell)
        .expect("render succeeds");
    match vdom {
        UiNode::Text(t) => assert_eq!(t.content.as_str(), "Total: 7"),
        other => panic!("expected text node, got {other:?}"),
    }
}

#[test]
fn callback_equality_for_props() {
    let cb1 = Callback::<i32>::noop();
    let cb2 = cb1.clone();
    let cb3 = Callback::<i32>::noop();

    let props1 = ChildButtonProps { on_click: cb1 };
    let props2 = ChildButtonProps { on_click: cb2 };
    let props3 = ChildButtonProps { on_click: cb3 };

    assert!(props1.props_eq(&props2));
    assert!(!props1.props_eq(&props3));
}
