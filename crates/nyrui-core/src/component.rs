//! Component authoring contracts, lifecycle hooks, and rendering contexts.
//!
//! This module implements the component model specified in **RFC-0001 §6.1**:
//! - [`Component`]: The core trait for stateful and stateless UI components.
//! - [`ShouldRender`]: Re-render decision indicator with boolean combinator algebra.
//! - [`ComponentContext`]: Lifecycle and update execution context exposing [`NodeId`](crate::id::NodeId) and invalidation.
//! - [`RenderContext`]: Render evaluation context.

pub mod context;
pub mod should_render;

pub use context::{ComponentContext, RenderContext};
pub use should_render::ShouldRender;

use crate::node::UiNode;
use crate::props::Props;

/// Core contract for declarative, stateful, and stateless UI components.
///
/// All components in Nyrui implement this trait as specified in **RFC-0001 §6.1**.
///
/// # Lifecycle Flow
///
/// 1. **Mounting:** When an instance is placed into the arena, [`Component::create`] is invoked,
///    followed immediately by [`Component::on_mount`].
/// 2. **Property Updates:** When the parent re-renders with new properties, [`Component::update_props`]
///    is called. If it returns [`ShouldRender::Yes`], a render pass is scheduled.
/// 3. **Message Updates:** When local events or asynchronous tasks dispatch a [`Component::Message`],
///    [`Component::update`] processes the message and indicates whether visual output changed.
/// 4. **Rendering:** [`Component::render`] builds the declarative [`UiNode`] subtree.
/// 5. **Unmounting:** When removed from the tree during reconciliation, [`Component::on_unmount`] cleans up.
pub trait Component: 'static + Sized {
    /// The properties type consumed by this component.
    type Props: Props;

    /// The component-local message type handled by [`Component::update`].
    type Message: 'static;

    /// Initializes a new component instance when mounted into the arena.
    fn create(props: &Self::Props, ctx: &mut ComponentContext<Self>) -> Self;

    /// Called when the parent re-renders with new [`Props`].
    ///
    /// Returns [`ShouldRender::Yes`] if the component needs to re-render,
    /// or [`ShouldRender::No`] to keep the existing visual subtree.
    ///
    /// The default implementation unconditionally returns [`ShouldRender::Yes`].
    fn update_props(
        &mut self,
        new_props: &Self::Props,
        ctx: &mut ComponentContext<Self>,
    ) -> ShouldRender {
        let _ = (new_props, ctx);
        ShouldRender::Yes
    }

    /// Processes a component-local message dispatched by an event handler or task.
    ///
    /// Returns whether the state mutation requires the component to re-render.
    /// The default implementation returns [`ShouldRender::No`].
    fn update(&mut self, msg: Self::Message, ctx: &mut ComponentContext<Self>) -> ShouldRender {
        let _ = (msg, ctx);
        ShouldRender::No
    }

    /// Produces the declarative [`UiNode`] description for the current state and props.
    fn render(&self, props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode;

    /// Lifecycle hook called immediately after the component is mounted into the tree arena.
    fn on_mount(&mut self, _ctx: &mut ComponentContext<Self>) {}

    /// Lifecycle hook called immediately before the component is unmounted and dropped.
    fn on_unmount(&mut self, _ctx: &mut ComponentContext<Self>) {}

    /// Lifecycle hook called when the component acquires keyboard focus.
    fn on_focus(&mut self, _ctx: &mut ComponentContext<Self>) {}

    /// Lifecycle hook called when the component loses keyboard focus.
    fn on_blur(&mut self, _ctx: &mut ComponentContext<Self>) {}
}
