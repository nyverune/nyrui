//! Declarative unmounted UI node tree representation.

use crate::id::{Key, NodeKey};
use crate::node::component::ComponentDescriptor;
use crate::node::text::TextNode;
use crate::props::{Props, SharedStr};

/// Declarative representation of an unmounted UI node.
///
/// `UiNode` is a transient value tree constructed during `render()` passes.
/// It separates visual intent from persistent runtime instances (RFC-0001 §4.1).
#[derive(Debug)]
pub enum UiNode {
    /// A managed component node described by a [`ComponentDescriptor`].
    Component(ComponentDescriptor),
    /// A leaf text element.
    Text(TextNode),
    /// A grouping of sibling nodes without an intermediate layout wrapper.
    Fragment(Vec<UiNode>),
    /// An empty node producing no visual output (useful for optional branches).
    Empty,
}

impl UiNode {
    /// Creates a component node for component type `C` with the given properties.
    #[inline]
    #[must_use]
    pub fn component<C, P>(props: P) -> Self
    where
        C: 'static,
        P: Props,
    {
        UiNode::Component(ComponentDescriptor::new::<C, P>(
            props,
            NodeKey::positional(0, 0),
        ))
    }

    /// Creates a leaf text node from any string-like value.
    #[inline]
    #[must_use]
    pub fn text(content: impl Into<SharedStr>) -> Self {
        UiNode::Text(TextNode::new(content))
    }

    /// Creates a fragment grouping multiple sibling nodes.
    #[inline]
    #[must_use]
    pub fn fragment(children: impl IntoIterator<Item = UiNode>) -> Self {
        UiNode::Fragment(children.into_iter().collect())
    }

    /// Creates an empty UI node.
    #[inline]
    #[must_use]
    pub const fn empty() -> Self {
        UiNode::Empty
    }

    /// Returns `true` if this node is [`UiNode::Empty`].
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(self, UiNode::Empty)
    }

    /// Returns a reference to the node's [`NodeKey`], if applicable.
    #[inline]
    #[must_use]
    pub fn key(&self) -> Option<&NodeKey> {
        match self {
            UiNode::Component(desc) => Some(&desc.key),
            UiNode::Text(text) => Some(&text.key),
            UiNode::Fragment(_) | UiNode::Empty => None,
        }
    }

    /// Sets a [`NodeKey`] (explicit or positional) on this node.
    #[inline]
    #[must_use]
    pub fn with_node_key(mut self, key: NodeKey) -> Self {
        match &mut self {
            UiNode::Component(desc) => {
                desc.key = key;
            }
            UiNode::Text(text) => {
                text.key = key;
            }
            UiNode::Fragment(children) => {
                if let Some(first) = children.first_mut() {
                    *first = std::mem::replace(first, UiNode::Empty).with_node_key(key);
                }
            }
            UiNode::Empty => {}
        }
        self
    }

    /// Assigns an explicit developer-provided [`Key`] to this node.
    ///
    /// If invoked on a [`UiNode::Fragment`], it assigns the key to the first child.
    #[inline]
    #[must_use]
    pub fn with_key(self, key: impl Into<Key>) -> Self {
        self.with_node_key(NodeKey::explicit(key))
    }

    /// Sets the compile-time branch `scope` ID for positional reconciliation.
    ///
    /// If the node has an explicit key, the explicit key takes precedence and is kept.
    /// Positional keys in conditional branches (`if`/`else`) receive distinct `scope` IDs
    /// so they never alias state across branches (RFC-0001 §6.3).
    #[inline]
    #[must_use]
    pub fn with_scope(mut self, scope: u16) -> Self {
        match &mut self {
            UiNode::Component(desc) => {
                if let NodeKey::Positional { index, .. } = desc.key {
                    desc.key = NodeKey::positional(scope, index);
                }
            }
            UiNode::Text(text) => {
                if let NodeKey::Positional { index, .. } = text.key {
                    text.key = NodeKey::positional(scope, index);
                }
            }
            UiNode::Fragment(children) => {
                for (idx, child) in children.iter_mut().enumerate() {
                    let child_node = std::mem::replace(child, UiNode::Empty);
                    *child = child_node
                        .with_scope(scope)
                        .with_positional_index(idx as u32);
                }
            }
            UiNode::Empty => {}
        }
        self
    }

    /// Helper to assign a positional child index within the active scope.
    #[inline]
    #[must_use]
    pub fn with_positional_index(mut self, new_index: u32) -> Self {
        match &mut self {
            UiNode::Component(desc) => {
                if let NodeKey::Positional { scope, .. } = desc.key {
                    desc.key = NodeKey::positional(scope, new_index);
                }
            }
            UiNode::Text(text) => {
                if let NodeKey::Positional { scope, .. } = text.key {
                    text.key = NodeKey::positional(scope, new_index);
                }
            }
            UiNode::Fragment(_) | UiNode::Empty => {}
        }
        self
    }
}

impl Default for UiNode {
    #[inline]
    fn default() -> Self {
        UiNode::Empty
    }
}

impl From<TextNode> for UiNode {
    #[inline]
    fn from(text: TextNode) -> Self {
        UiNode::Text(text)
    }
}

impl From<ComponentDescriptor> for UiNode {
    #[inline]
    fn from(desc: ComponentDescriptor) -> Self {
        UiNode::Component(desc)
    }
}

impl From<SharedStr> for UiNode {
    #[inline]
    fn from(s: SharedStr) -> Self {
        UiNode::text(s)
    }
}

impl From<&'static str> for UiNode {
    #[inline]
    fn from(s: &'static str) -> Self {
        UiNode::text(s)
    }
}

impl From<String> for UiNode {
    #[inline]
    fn from(s: String) -> Self {
        UiNode::text(s)
    }
}

impl From<Vec<UiNode>> for UiNode {
    #[inline]
    fn from(nodes: Vec<UiNode>) -> Self {
        UiNode::Fragment(nodes)
    }
}

impl From<Option<UiNode>> for UiNode {
    #[inline]
    fn from(opt: Option<UiNode>) -> Self {
        opt.unwrap_or(UiNode::Empty)
    }
}

impl From<()> for UiNode {
    #[inline]
    fn from((): ()) -> Self {
        UiNode::Empty
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
    fn uinode_constructors_and_properties() {
        let empty = UiNode::empty();
        assert!(empty.is_empty());
        assert_eq!(empty.key(), None);

        let text = UiNode::text("hello");
        assert!(!text.is_empty());
        assert_eq!(text.key(), Some(&NodeKey::positional(0, 0)));

        let comp = UiNode::component::<MockComponent, _>(MockProps { title: "title" });
        assert_eq!(comp.key(), Some(&NodeKey::positional(0, 0)));
    }

    #[test]
    fn uinode_with_key_fluent_method() {
        let text = UiNode::text("hello").with_key("my_key");
        assert_eq!(text.key(), Some(&NodeKey::explicit("my_key")));

        let comp =
            UiNode::component::<MockComponent, _>(MockProps { title: "title" }).with_key(42u64);
        assert_eq!(comp.key(), Some(&NodeKey::explicit(42u64)));
    }

    #[test]
    fn uinode_with_scope_fluent_method() {
        let text = UiNode::text("branch").with_scope(5);
        assert_eq!(text.key(), Some(&NodeKey::positional(5, 0)));

        // Explicit keys are preserved when with_scope is called
        let explicit = UiNode::text("explicit").with_key("fixed").with_scope(5);
        assert_eq!(explicit.key(), Some(&NodeKey::explicit("fixed")));
    }

    #[test]
    fn uinode_fragment_scope_propagation() {
        let fragment =
            UiNode::fragment([UiNode::text("child0"), UiNode::text("child1")]).with_scope(3);

        if let UiNode::Fragment(children) = fragment {
            assert_eq!(children[0].key(), Some(&NodeKey::positional(3, 0)));
            assert_eq!(children[1].key(), Some(&NodeKey::positional(3, 1)));
        } else {
            panic!("expected Fragment");
        }
    }
}
