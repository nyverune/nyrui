//! Integration tests for the Component trait, ShouldRender, ComponentContext, and RenderContext.

use nyrui_core::component::{Component, ComponentContext, RenderContext, ShouldRender};
use nyrui_core::id::NodeId;
use nyrui_core::node::UiNode;
use nyrui_core::props::{Props, SharedStr};

use std::cell::Cell;
use std::num::NonZeroU32;

#[derive(Clone, Debug, PartialEq, Eq)]
struct CounterProps {
    title: SharedStr,
    step: i32,
}

impl Props for CounterProps {
    fn props_eq(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CounterMessage {
    Increment,
    Decrement,
    Reset,
}

#[derive(Debug)]
struct MockCounter {
    count: i32,
    step: i32,
    mounted: bool,
    unmounted: bool,
    focused: bool,
}

impl Component for MockCounter {
    type Props = CounterProps;
    type Message = CounterMessage;

    fn create(props: &Self::Props, ctx: &mut ComponentContext<Self>) -> Self {
        assert_eq!(ctx.node_id().index, 10);
        Self {
            count: 0,
            step: props.step,
            mounted: false,
            unmounted: false,
            focused: false,
        }
    }

    fn update_props(
        &mut self,
        new_props: &Self::Props,
        _ctx: &mut ComponentContext<Self>,
    ) -> ShouldRender {
        if self.step != new_props.step {
            self.step = new_props.step;
            ShouldRender::Yes
        } else {
            ShouldRender::No
        }
    }

    fn update(&mut self, msg: Self::Message, ctx: &mut ComponentContext<Self>) -> ShouldRender {
        match msg {
            CounterMessage::Increment => {
                self.count += self.step;
                ctx.invalidate();
                ShouldRender::Yes
            }
            CounterMessage::Decrement => {
                self.count -= self.step;
                ctx.invalidate();
                ShouldRender::Yes
            }
            CounterMessage::Reset => {
                if self.count != 0 {
                    self.count = 0;
                    ctx.invalidate();
                    ShouldRender::Yes
                } else {
                    ShouldRender::No
                }
            }
        }
    }

    fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
        assert_eq!(ctx.node_id().index, 10);
        UiNode::text(format!("{}: {}", props.title, self.count))
    }

    fn on_mount(&mut self, _ctx: &mut ComponentContext<Self>) {
        self.mounted = true;
    }

    fn on_unmount(&mut self, _ctx: &mut ComponentContext<Self>) {
        self.unmounted = true;
    }

    fn on_focus(&mut self, _ctx: &mut ComponentContext<Self>) {
        self.focused = true;
    }

    fn on_blur(&mut self, _ctx: &mut ComponentContext<Self>) {
        self.focused = false;
    }
}

fn test_node_id() -> NodeId {
    NodeId::new(10, NonZeroU32::new(1).expect("non-zero generation"))
}

#[test]
fn component_lifecycle_and_updates() {
    let node_id = test_node_id();
    let dirty_cell = Cell::new(false);
    let mut comp_ctx = ComponentContext::<MockCounter>::new(node_id, &dirty_cell);

    let initial_props = CounterProps {
        title: SharedStr::from("Active Count"),
        step: 5,
    };

    // 1. Creation
    let mut counter = MockCounter::create(&initial_props, &mut comp_ctx);
    assert_eq!(counter.count, 0);
    assert_eq!(counter.step, 5);
    assert!(!counter.mounted);
    assert!(!counter.unmounted);
    assert!(!counter.focused);

    // 2. Mount hook
    counter.on_mount(&mut comp_ctx);
    assert!(counter.mounted);

    // 3. Render
    let render_cell = Cell::new(false);
    let render_ctx = RenderContext::<MockCounter>::new(node_id, &render_cell);
    let vdom = counter.render(&initial_props, &render_ctx);
    match vdom {
        UiNode::Text(t) => assert_eq!(t.content.as_str(), "Active Count: 0"),
        other => panic!("expected Text node, got {other:?}"),
    }

    // 4. Message updates
    assert!(!dirty_cell.get());
    let should_render = counter.update(CounterMessage::Increment, &mut comp_ctx);
    assert_eq!(should_render, ShouldRender::Yes);
    assert_eq!(counter.count, 5);
    assert!(dirty_cell.get());

    // Reset dirty flag
    dirty_cell.set(false);

    let should_render_dec = counter.update(CounterMessage::Decrement, &mut comp_ctx);
    assert_eq!(should_render_dec, ShouldRender::Yes);
    assert_eq!(counter.count, 0);
    assert!(dirty_cell.get());

    // Resetting when already zero returns ShouldRender::No
    dirty_cell.set(false);
    let should_render_reset = counter.update(CounterMessage::Reset, &mut comp_ctx);
    assert_eq!(should_render_reset, ShouldRender::No);
    assert!(!dirty_cell.get());

    // 5. Props update
    let identical_props = CounterProps {
        title: SharedStr::from("Active Count"),
        step: 5,
    };
    let update_res = counter.update_props(&identical_props, &mut comp_ctx);
    assert_eq!(update_res, ShouldRender::No);

    let changed_props = CounterProps {
        title: SharedStr::from("Active Count"),
        step: 10,
    };
    let update_res2 = counter.update_props(&changed_props, &mut comp_ctx);
    assert_eq!(update_res2, ShouldRender::Yes);
    assert_eq!(counter.step, 10);

    // 6. Focus hooks
    assert!(!counter.focused);
    counter.on_focus(&mut comp_ctx);
    assert!(counter.focused);
    counter.on_blur(&mut comp_ctx);
    assert!(!counter.focused);

    // 7. Unmount hook
    assert!(!counter.unmounted);
    counter.on_unmount(&mut comp_ctx);
    assert!(counter.unmounted);
}

#[test]
fn should_render_algebra() {
    assert!(ShouldRender::Yes.is_yes());
    assert!(!ShouldRender::Yes.is_no());
    assert!(ShouldRender::No.is_no());
    assert!(!ShouldRender::No.is_yes());

    assert_eq!(ShouldRender::Yes | ShouldRender::No, ShouldRender::Yes);
    assert_eq!(ShouldRender::No | ShouldRender::No, ShouldRender::No);
    assert_eq!(ShouldRender::Yes & ShouldRender::No, ShouldRender::No);
    assert_eq!(ShouldRender::Yes & ShouldRender::Yes, ShouldRender::Yes);

    assert_eq!(!ShouldRender::Yes, ShouldRender::No);
    assert_eq!(!ShouldRender::No, ShouldRender::Yes);

    assert_eq!(ShouldRender::from(true), ShouldRender::Yes);
    assert_eq!(ShouldRender::from(false), ShouldRender::No);
    assert!(bool::from(ShouldRender::Yes));
    assert!(!bool::from(ShouldRender::No));
}

#[test]
fn context_debug_format() {
    let node_id = test_node_id();
    let cell = Cell::new(true);
    let comp_ctx = ComponentContext::<MockCounter>::new(node_id, &cell);
    let formatted = format!("{comp_ctx:?}");
    assert!(formatted.contains("ComponentContext"));
    assert!(formatted.contains("is_invalidated: true"));

    let render_ctx = RenderContext::<MockCounter>::new(node_id, &cell);
    let formatted_r = format!("{render_ctx:?}");
    assert!(formatted_r.contains("RenderContext"));
    assert!(formatted_r.contains("is_invalidated: true"));
}
