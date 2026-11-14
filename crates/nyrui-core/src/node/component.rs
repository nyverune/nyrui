//! Type-erased component description for unmounted visual trees.

use crate::id::NodeKey;
use crate::props::Props;

use std::any::{Any, TypeId, type_name};
use std::fmt;

/// A type-erased declarative description of a component awaiting mounting or reconciliation.
///
/// Contains the component's [`TypeId`], its user-assigned or positional [`NodeKey`],
/// and its boxed [`Props`].
pub struct ComponentDescriptor {
    /// The unique [`TypeId`] of the component.
    pub type_id: TypeId,
    /// Human-readable type name for debugging and diagnostics.
    pub type_name: &'static str,
    /// Stable reconciliation key (explicit or positional).
    pub key: NodeKey,
    /// Boxed, type-erased component properties.
    pub props: Box<dyn Any>,
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
        }
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
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockComponent;

    #[derive(Debug, PartialEq, Eq)]
    struct MockProps {
        pub title: &'static str,
    }

    impl Props for MockProps {}

    #[test]
    fn component_descriptor_creation_and_downcast() {
        let desc = ComponentDescriptor::new::<MockComponent, _>(
            MockProps { title: "Test" },
            NodeKey::positional(0, 0),
        );

        assert_eq!(desc.type_id, TypeId::of::<MockComponent>());
        assert!(desc.type_name.contains("MockComponent"));
        let props = desc
            .downcast_props::<MockProps>()
            .expect("props should match MockProps");
        assert_eq!(props.title, "Test");
    }
}
