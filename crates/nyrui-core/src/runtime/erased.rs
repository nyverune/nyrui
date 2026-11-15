//! Type-erased component wrappers and blanket adapters.

use crate::component::{Component, ComponentContext, RenderContext, ShouldRender};
use crate::node::UiNode;
use crate::props::Props;

use std::any::{Any, TypeId};
use std::fmt;

/// Type-erased trait interface for heterogeneous components stored in the arena.
///
/// Implements the object-safe component abstraction defined in **RFC-0001 §6.2**.
/// Concrete component instances pair with their current `Props` and are accessed
/// through this trait during reconciliation, message dispatch, and rendering passes.
pub trait ErasedComponent: fmt::Debug {
    /// Returns the unique [`TypeId`] of the underlying component.
    fn type_id(&self) -> TypeId;

    /// Returns the human-readable type name of the underlying component.
    fn type_name(&self) -> &'static str;

    /// Returns whether the provided type-erased props are equal to the current props
    /// according to [`Props::props_eq`].
    fn props_eq_erased(&self, other_props: &dyn Any) -> bool;

    /// Updates the component with new type-erased properties.
    ///
    /// # Errors
    ///
    /// Returns `Err(new_props)` if the provided boxed props cannot be downcast
    /// to the component's concrete `Props` type.
    fn update_props_erased(
        &mut self,
        new_props: Box<dyn Any>,
        ctx: &mut ComponentContext<()>,
    ) -> Result<ShouldRender, Box<dyn Any>>;

    /// Dispatches a type-erased message to the component's [`Component::update`].
    ///
    /// # Errors
    ///
    /// Returns `Err(msg)` if the provided boxed message cannot be downcast
    /// to the component's concrete `Message` type.
    fn update_erased(
        &mut self,
        msg: Box<dyn Any>,
        ctx: &mut ComponentContext<()>,
    ) -> Result<ShouldRender, Box<dyn Any>>;

    /// Invokes the component's [`Component::render`] method.
    fn render_erased(&self, ctx: &RenderContext<()>) -> UiNode;

    /// Lifecycle hook: component mounted into arena.
    fn on_mount(&mut self, ctx: &mut ComponentContext<()>);

    /// Lifecycle hook: component unmounted from arena.
    fn on_unmount(&mut self, ctx: &mut ComponentContext<()>);

    /// Lifecycle hook: component acquired keyboard focus.
    fn on_focus(&mut self, ctx: &mut ComponentContext<()>);

    /// Lifecycle hook: component lost keyboard focus.
    fn on_blur(&mut self, ctx: &mut ComponentContext<()>);

    /// Downcasts a reference to the inner concrete component if types match.
    fn as_any(&self) -> &dyn Any;

    /// Downcasts a mutable reference to the inner concrete component if types match.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// A typed pair of a concrete [`Component`] and its current [`Component::Props`].
///
/// Automatically implements [`ErasedComponent`] for any `C: Component`.
pub struct ComponentInstance<C: Component> {
    /// The concrete component instance.
    pub component: C,
    /// The current properties bound to this component.
    pub props: C::Props,
}

impl<C: Component> ComponentInstance<C> {
    /// Wraps a concrete component and its properties into a [`ComponentInstance`].
    #[inline]
    #[must_use]
    pub fn new(component: C, props: C::Props) -> Self {
        Self { component, props }
    }
}

impl<C: Component> fmt::Debug for ComponentInstance<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentInstance")
            .field("type_name", &std::any::type_name::<C>())
            .field("type_id", &TypeId::of::<C>())
            .finish_non_exhaustive()
    }
}

impl<C: Component> ErasedComponent for ComponentInstance<C> {
    #[inline]
    fn type_id(&self) -> TypeId {
        TypeId::of::<C>()
    }

    #[inline]
    fn type_name(&self) -> &'static str {
        std::any::type_name::<C>()
    }

    #[inline]
    fn props_eq_erased(&self, other_props: &dyn Any) -> bool {
        other_props
            .downcast_ref::<C::Props>()
            .is_some_and(|p| self.props.props_eq(p))
    }

    #[inline]
    fn update_props_erased(
        &mut self,
        new_props: Box<dyn Any>,
        ctx: &mut ComponentContext<()>,
    ) -> Result<ShouldRender, Box<dyn Any>> {
        match new_props.downcast::<C::Props>() {
            Ok(boxed_props) => {
                let mut typed_ctx = ctx.rebind::<C>();
                let decision = self.component.update_props(&boxed_props, &mut typed_ctx);
                self.props = *boxed_props;
                Ok(decision)
            }
            Err(err) => Err(err),
        }
    }

    #[inline]
    fn update_erased(
        &mut self,
        msg: Box<dyn Any>,
        ctx: &mut ComponentContext<()>,
    ) -> Result<ShouldRender, Box<dyn Any>> {
        match msg.downcast::<C::Message>() {
            Ok(boxed_msg) => {
                let mut typed_ctx = ctx.rebind::<C>();
                let decision = self.component.update(*boxed_msg, &mut typed_ctx);
                Ok(decision)
            }
            Err(err) => Err(err),
        }
    }

    #[inline]
    fn render_erased(&self, ctx: &RenderContext<()>) -> UiNode {
        let typed_ctx = ctx.rebind::<C>();
        self.component.render(&self.props, &typed_ctx)
    }

    #[inline]
    fn on_mount(&mut self, ctx: &mut ComponentContext<()>) {
        let mut typed_ctx = ctx.rebind::<C>();
        self.component.on_mount(&mut typed_ctx);
    }

    #[inline]
    fn on_unmount(&mut self, ctx: &mut ComponentContext<()>) {
        let mut typed_ctx = ctx.rebind::<C>();
        self.component.on_unmount(&mut typed_ctx);
    }

    #[inline]
    fn on_focus(&mut self, ctx: &mut ComponentContext<()>) {
        let mut typed_ctx = ctx.rebind::<C>();
        self.component.on_focus(&mut typed_ctx);
    }

    #[inline]
    fn on_blur(&mut self, ctx: &mut ComponentContext<()>) {
        let mut typed_ctx = ctx.rebind::<C>();
        self.component.on_blur(&mut typed_ctx);
    }

    #[inline]
    fn as_any(&self) -> &dyn Any {
        &self.component
    }

    #[inline]
    fn as_any_mut(&mut self) -> &mut dyn Any {
        &mut self.component
    }
}
