//! Stack-allocated small children collection for declarative layouts.

use crate::node::ui_node::UiNode;

use smallvec::SmallVec;

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

impl From<Children> for UiNode {
    #[inline]
    fn from(children: Children) -> Self {
        children.into_node()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
