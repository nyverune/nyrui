//! Declarative UI Description Layer.
//!
//! This module implements the intermediate representation tree specified in
//! **RFC-0001 §4.1 and §5.3**:
//! - [`UiNode`]: Declarative description of an unmounted visual node (component, text, fragment, or empty).
//! - [`ComponentDescriptor`]: Type-erased description of a component, its properties, and its identity key.
//! - [`TextNode`]: Lightweight text node holding a [`SharedStr`].
//! - [`Children`]: Small-vector-backed container optimizing child collections without heap allocations for small trees.
//! - [`SlotFn`]: Single-threaded scoped slot closure generating dynamic subtrees from contextual data.

use crate::id::{Key, NodeKey};
use crate::props::{Props, SharedStr};

use smallvec::SmallVec;
use std::any::{Any, TypeId, type_name};
use std::fmt;
use std::rc::Rc;

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

/// A declarative text node holding a [`SharedStr`] and a [`NodeKey`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextNode {
    /// Text content to be rendered.
    pub content: SharedStr,
    /// Stable reconciliation key.
    pub key: NodeKey,
}

impl TextNode {
    /// Creates a new [`TextNode`] with default positional identity.
    #[inline]
    #[must_use]
    pub fn new(content: impl Into<SharedStr>) -> Self {
        Self {
            content: content.into(),
            key: NodeKey::positional(0, 0),
        }
    }
}

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

impl From<Children> for UiNode {
    #[inline]
    fn from(children: Children) -> Self {
        children.into_node()
    }
}

impl From<()> for UiNode {
    #[inline]
    fn from((): ()) -> Self {
        UiNode::Empty
    }
}

/// A container for child UI nodes backed by a [`SmallVec`].
///
/// Small collections ($\le 4$ children) are stored inline on the stack without heap allocation,
/// drastically reducing allocator pressure during declarative view construction.
#[derive(Debug, Default)]
pub struct Children(pub SmallVec<[UiNode; 4]>);

impl Children {
    /// Creates an empty [`Children`] container.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self(SmallVec::new())
    }

    /// Creates an empty [`Children`] container with heap capacity for at least `capacity` elements.
    #[inline]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self(SmallVec::with_capacity(capacity))
    }

    /// Appends a child node to the collection.
    #[inline]
    pub fn push(&mut self, child: impl Into<UiNode>) {
        self.0.push(child.into());
    }

    /// Fluent builder method to append a child node.
    #[inline]
    #[must_use]
    pub fn push_child(mut self, child: impl Into<UiNode>) -> Self {
        self.push(child);
        self
    }

    /// Extends the children collection with an iterator of nodes.
    #[inline]
    pub fn extend(&mut self, iter: impl IntoIterator<Item = UiNode>) {
        self.0.extend(iter);
    }

    /// Returns the number of child nodes.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if there are no child nodes.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns a slice of the child nodes.
    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[UiNode] {
        self.0.as_slice()
    }

    /// Returns a mutable slice of the child nodes.
    #[inline]
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [UiNode] {
        self.0.as_mut_slice()
    }

    /// Consumes the container and returns the inner nodes as a standard [`Vec`].
    #[inline]
    #[must_use]
    pub fn into_vec(self) -> Vec<UiNode> {
        self.0.into_vec()
    }

    /// Converts the children collection directly into a [`UiNode::Fragment`].
    #[inline]
    #[must_use]
    pub fn into_node(self) -> UiNode {
        UiNode::Fragment(self.into_vec())
    }
}

impl std::ops::Deref for Children {
    type Target = [UiNode];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl std::ops::DerefMut for Children {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl IntoIterator for Children {
    type Item = UiNode;
    type IntoIter = smallvec::IntoIter<[UiNode; 4]>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Children {
    type Item = &'a UiNode;
    type IntoIter = std::slice::Iter<'a, UiNode>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl FromIterator<UiNode> for Children {
    #[inline]
    fn from_iter<T: IntoIterator<Item = UiNode>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl From<Vec<UiNode>> for Children {
    #[inline]
    fn from(nodes: Vec<UiNode>) -> Self {
        Self(SmallVec::from_vec(nodes))
    }
}

impl From<SmallVec<[UiNode; 4]>> for Children {
    #[inline]
    fn from(vec: SmallVec<[UiNode; 4]>) -> Self {
        Self(vec)
    }
}

impl From<UiNode> for Children {
    #[inline]
    fn from(node: UiNode) -> Self {
        let mut children = Children::new();
        children.push(node);
        children
    }
}

/// A single-threaded scoped slot closure for dynamic list or table rendering (RFC-0001 §5.3).
///
/// Because Nyrui's runtime arena is single-threaded (`!Send`), `SlotFn` wraps an [`Rc`]
/// closure rather than requiring `Arc<dyn Fn + Send + Sync>`.
pub struct SlotFn<T: ?Sized>(Rc<dyn Fn(&T) -> UiNode>);

impl<T: ?Sized> Clone for SlotFn<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: ?Sized> SlotFn<T> {
    /// Creates a new scoped slot closure.
    #[inline]
    #[must_use]
    pub fn new(f: impl Fn(&T) -> UiNode + 'static) -> Self {
        Self(Rc::new(f))
    }

    /// Invokes the slot closure with the provided item reference, generating a [`UiNode`].
    #[inline]
    #[must_use]
    pub fn call(&self, item: &T) -> UiNode {
        (self.0)(item)
    }

    /// Alias for [`SlotFn::call`] for template rendering ergonomics.
    #[inline]
    #[must_use]
    pub fn render(&self, item: &T) -> UiNode {
        self.call(item)
    }
}

impl<T: ?Sized> fmt::Debug for SlotFn<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlotFn").finish_non_exhaustive()
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
    fn component_descriptor_downcast() {
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

        // If explicit key is set, with_scope does not overwrite it:
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

    #[test]
    fn children_inline_smallvec_and_methods() {
        let mut children = Children::new();
        assert!(children.is_empty());
        assert_eq!(children.len(), 0);

        children.push(UiNode::text("item1"));
        children.push(UiNode::text("item2"));
        assert_eq!(children.len(), 2);
        assert!(!children.is_empty());

        let node: UiNode = children.into_node();
        if let UiNode::Fragment(vec) = node {
            assert_eq!(vec.len(), 2);
        } else {
            panic!("expected Fragment");
        }
    }

    #[test]
    fn slot_fn_evaluation_and_cloning() {
        struct User {
            pub id: u64,
            pub name: &'static str,
        }

        let slot = SlotFn::new(|user: &User| UiNode::text(format!("{}: {}", user.id, user.name)));

        let slot_clone = slot.clone();

        let user = User {
            id: 101,
            name: "Alice",
        };
        let rendered = slot_clone.render(&user);

        if let UiNode::Text(text) = rendered {
            assert_eq!(text.content, "101: Alice");
        } else {
            panic!("expected Text node");
        }
    }
}
