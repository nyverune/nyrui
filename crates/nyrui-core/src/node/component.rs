//! Type-erased component description for unmounted visual trees.

use crate::component::Component;
use crate::component::context::ComponentContext;
use crate::id::NodeKey;
use crate::props::Props;
use crate::runtime::erased::{ComponentInstance, ErasedComponent};

use std::any::{Any, TypeId, type_name};
use std::fmt;

/// Factory closure used to instantiate a component when mounting into the arena.
pub type ComponentFactory =
    Box<dyn FnOnce(Box<dyn Any>, &mut ComponentContext<()>) -> Box<dyn ErasedComponent>>;

/// A type-erased declarative description of a component awaiting mounting or reconciliation.
///
/// Contains the component's [`TypeId`], its user-assigned or positional [`NodeKey`],
/// its boxed [`Props`], and an optional component factory closure.
pub struct ComponentDescriptor {
    /// The unique [`TypeId`] of the component.
    pub type_id: TypeId,
    /// Human-readable type name for debugging and diagnostics.
    pub type_name: &'static str,
    /// Stable reconciliation key (explicit or positional).
    pub key: NodeKey,
    /// Boxed, type-erased component properties.
    pub props: Box<dyn Any>,
    /// Factory closure to instantiate the component if newly mounted.
    pub factory: Option<ComponentFactory>,
}

impl ComponentDescriptor {
    /// Creates a new [`ComponentDescriptor`] for a component type `C` and properties `P`.
    #[inline]
    #[must_use]
    pub fn new<C, P>(props: P, key: NodeKey) -> Self
    where
        C: 'static,
        P: Props,
    {
        Self {
            type_id: TypeId::of::<C>(),
            type_name: type_name::<C>(),
            key,
            props: Box::new(props),
            factory: None,
        }
    }

    /// Creates a [`ComponentDescriptor`] with a factory for component type `C`.
    #[inline]
    #[must_use]
    pub fn of<C>(props: C::Props, key: NodeKey) -> Self
    where
        C: Component,
    {
        Self {
            type_id: TypeId::of::<C>(),
            type_name: type_name::<C>(),
            key,
            props: Box::new(props),
            factory: Some(Box::new(|boxed_props, ctx| {
                let p = boxed_props
                    .downcast::<C::Props>()
                    .expect("valid props type");
                let mut typed_ctx = ctx.rebind::<C>();
                let comp = C::create(&p, &mut typed_ctx);
                Box::new(ComponentInstance::new(comp, *p))
            })),
        }
    }

    /// Attaches a factory closure to this descriptor.
    #[inline]
    #[must_use]
    pub fn with_factory(mut self, factory: ComponentFactory) -> Self {
        self.factory = Some(factory);
        self
    }

    /// Downcasts a reference to the inner props if the requested type matches.
    #[inline]
    #[must_use]
    pub fn downcast_props<P: Props>(&self) -> Option<&P> {
        self.props.downcast_ref::<P>()
    }
}

impl fmt::Debug for ComponentDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentDescriptor")
            .field("type_name", &self.type_name)
            .field("type_id", &self.type_id)
            .field("key", &self.key)
            .field("has_factory", &self.factory.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy;

    #[derive(Debug, PartialEq, Eq)]
    struct DummyProps(u32);

    impl Props for DummyProps {}

    #[test]
    fn component_descriptor_creation_and_downcast() {
        let descriptor =
            ComponentDescriptor::new::<Dummy, _>(DummyProps(42), NodeKey::positional(0, 0));

        assert_eq!(descriptor.type_id, TypeId::of::<Dummy>());
        assert!(descriptor.downcast_props::<DummyProps>().is_some());
        assert_eq!(
            descriptor
                .downcast_props::<DummyProps>()
                .expect("downcast props must succeed")
                .0,
            42
        );
        assert!(descriptor.downcast_props::<()>().is_none());
    }
}
