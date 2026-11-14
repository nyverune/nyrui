//! Declarative text node wrapper.

use crate::id::NodeKey;
use crate::props::SharedStr;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_node_creation() {
        let node = TextNode::new("hello");
        assert_eq!(node.content, "hello");
        assert_eq!(node.key, NodeKey::positional(0, 0));
    }
}
