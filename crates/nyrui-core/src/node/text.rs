//! Declarative text node wrapper and primitive component.

use crate::component::{Component, ComponentContext, RenderContext, ShouldRender};
use crate::id::NodeKey;
use crate::node::UiNode;
use crate::node::component::ComponentDescriptor;
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

    /// Converts this text node into a [`ComponentDescriptor`] backed by [`TextComponent`].
    #[inline]
    #[must_use]
    pub fn into_descriptor(self) -> ComponentDescriptor {
        ComponentDescriptor::of::<TextComponent>(self.content, self.key)
    }
}

/// Built-in leaf component rendering text.
#[derive(Debug)]
pub struct TextComponent {
    /// Cached text content.
    pub content: SharedStr,
}

impl Component for TextComponent {
    type Props = SharedStr;
    type Message = ();

    fn create(props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self {
            content: props.clone(),
        }
    }

    fn update_props(
        &mut self,
        new_props: &Self::Props,
        _ctx: &mut ComponentContext<Self>,
    ) -> ShouldRender {
        if self.content != *new_props {
            self.content = new_props.clone();
            ShouldRender::Yes
        } else {
            ShouldRender::No
        }
    }

    fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        UiNode::text(self.content.clone())
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

        let descriptor = node.into_descriptor();
        assert_eq!(descriptor.type_name, std::any::type_name::<TextComponent>());
    }
}
