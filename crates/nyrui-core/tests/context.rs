//! Integration tests for typed context provider mechanism (RFC-0001 §11.2).
//!
//! Verifies:
//! - Root provisioning and descendant lookup via `provide_context` and `use_context`.
//! - Multi-level ancestor chain traversal in $O(\text{depth})$.
//! - Missing context returns `None`.
//! - Subtree shadowing where intermediate providers override ancestors for their descendants.
//! - Context accessibility in lifecycle hooks (`on_mount`, `render`, `update`).
//! - Context preservation and access across reconciliation passes.

use nyrui_core::component::{Component, ComponentContext, RenderContext, ShouldRender};
use nyrui_core::id::{Key, NodeKey};
use nyrui_core::node::{ComponentDescriptor, UiNode};
use nyrui_core::props::SharedStr;
use nyrui_core::runtime::arena::Arena;
use nyrui_core::runtime::reconcile::mount_node;

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ThemeContext {
    pub name: SharedStr,
    pub dark_mode: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ServiceContext {
    pub service_id: u64,
}

// ---------------------------------------------------------------------------
// Components for integration testing
// ---------------------------------------------------------------------------

struct RootProviderComponent {
    theme: ThemeContext,
    service: Option<ServiceContext>,
}

impl Component for RootProviderComponent {
    type Props = ();
    type Message = ();

    fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self {
            theme: ThemeContext {
                name: SharedStr::from_static("nord"),
                dark_mode: true,
            },
            service: Some(ServiceContext { service_id: 1001 }),
        }
    }

    fn on_mount(&mut self, ctx: &mut ComponentContext<Self>) {
        ctx.provide_context(self.theme.clone());
        if let Some(ref s) = self.service {
            ctx.provide_context(s.clone());
        }
    }

    fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        UiNode::component_of::<MiddleComponent>(()).with_key("middle")
    }
}

struct MiddleComponent {
    retrieved_theme: Option<ThemeContext>,
}

impl Component for MiddleComponent {
    type Props = ();
    type Message = ();

    fn create(_props: &Self::Props, _ctx: &mut ComponentContext<Self>) -> Self {
        Self {
            retrieved_theme: None,
        }
    }

    fn on_mount(&mut self, ctx: &mut ComponentContext<Self>) {
        self.retrieved_theme = ctx.use_context::<ThemeContext>();
    }

    fn render(&self, _props: &Self::Props, _ctx: &RenderContext<Self>) -> UiNode {
        UiNode::component_of::<LeafConsumerComponent>(()).with_key("leaf")
    }
}

struct LeafConsumerComponent {
    pub theme: Option<ThemeContext>,
    pub service: Option<ServiceContext>,
}

impl Component for LeafConsumerComponent {
    type Props = ();
    type Message = ();

    fn create(_props: &Self::Props, ctx: &mut ComponentContext<Self>) -> Self {
        Self {
            theme: ctx.use_context::<ThemeContext>(),
            service: ctx.use_context::<ServiceContext>(),
        }
    }

    fn on_mount(&mut self, ctx: &mut ComponentContext<Self>) {
        // Re-verify in on_mount
        self.theme = ctx.use_context::<ThemeContext>();
        self.service = ctx.use_context::<ServiceContext>();
    }

    fn render(&self, _props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
        let theme_text = match ctx.use_context::<ThemeContext>() {
            Some(t) => format!("Theme: {}", t.name.as_str()),
            None => "Theme: None".to_string(),
        };
        UiNode::text(theme_text)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn root_provisioning_and_descendant_lookup() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    let desc =
        ComponentDescriptor::of::<RootProviderComponent>((), NodeKey::Explicit(Key::from("root")));
    let root_id = mount_node(&mut arena, None, 0, desc, &dirty_cell);

    let root_node = arena.get(root_id).expect("root node exists");
    assert_eq!(root_node.children.len(), 1);
    let middle_id = root_node.children[0];

    let middle_node = arena.get(middle_id).expect("middle node exists");
    assert_eq!(middle_node.children.len(), 1);
    let leaf_id = middle_node.children[0];

    // Verify context accessible via Arena::use_context
    let theme_at_leaf = arena.use_context::<ThemeContext>(leaf_id);
    assert_eq!(
        theme_at_leaf,
        Some(ThemeContext {
            name: SharedStr::from_static("nord"),
            dark_mode: true,
        })
    );

    let service_at_leaf = arena.use_context::<ServiceContext>(leaf_id);
    assert_eq!(service_at_leaf, Some(ServiceContext { service_id: 1001 }));

    // Verify missing context returns None
    let missing_at_leaf = arena.use_context::<String>(leaf_id);
    assert_eq!(missing_at_leaf, None);

    // Verify RenderContext lookup produced expected visual text node
    let rendered_ui = arena.render(leaf_id, &dirty_cell).expect("render succeeds");
    match rendered_ui {
        UiNode::Text(t) => assert_eq!(t.content.as_str(), "Theme: nord"),
        other => panic!("expected text node, got {other:?}"),
    }
}

#[test]
fn multi_level_deep_hierarchy_lookup() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    #[derive(Clone, Debug, PartialEq)]
    struct DeepContext(usize);

    struct DeepParent;
    impl Component for DeepParent {
        type Props = ();
        type Message = ();
        fn create(_: &Self::Props, _: &mut ComponentContext<Self>) -> Self {
            Self
        }
        fn render(&self, _: &Self::Props, _: &RenderContext<Self>) -> UiNode {
            UiNode::empty()
        }
    }

    let mut current_parent = None;
    let mut node_ids = Vec::new();

    // Build chain of 10 nodes: 0 -> 1 -> 2 -> ... -> 9
    for i in 0..10_i32 {
        let desc = ComponentDescriptor::of::<DeepParent>((), NodeKey::Explicit(Key::from(i)));
        let id = mount_node(&mut arena, current_parent, i as u16, desc, &dirty_cell);
        current_parent = Some(id);
        node_ids.push(id);
    }

    // Provide context at root (node 0)
    let root_ctx = ComponentContext::<DeepParent>::for_arena(node_ids[0], &arena);
    root_ctx.provide_context(DeepContext(42));

    // Descendant at depth 9 should find DeepContext(42) in O(depth)
    let leaf_ctx = ComponentContext::<DeepParent>::for_arena(node_ids[9], &arena);
    assert_eq!(leaf_ctx.use_context::<DeepContext>(), Some(DeepContext(42)));

    // Intermediate node at depth 5 should also find it
    let mid_ctx = ComponentContext::<DeepParent>::for_arena(node_ids[5], &arena);
    assert_eq!(mid_ctx.use_context::<DeepContext>(), Some(DeepContext(42)));
}

#[test]
fn missing_context_returns_none() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    struct EmptyComp;
    impl Component for EmptyComp {
        type Props = ();
        type Message = ();
        fn create(_: &Self::Props, _: &mut ComponentContext<Self>) -> Self {
            Self
        }
        fn render(&self, _: &Self::Props, _: &RenderContext<Self>) -> UiNode {
            UiNode::empty()
        }
    }

    let desc = ComponentDescriptor::of::<EmptyComp>((), NodeKey::Explicit(Key::from("test")));
    let id = mount_node(&mut arena, None, 0, desc, &dirty_cell);

    let ctx = ComponentContext::<EmptyComp>::for_arena(id, &arena);
    assert_eq!(ctx.use_context::<ThemeContext>(), None);
    assert_eq!(ctx.use_context::<u32>(), None);
    assert!(ctx.use_context::<Arc<AtomicUsize>>().is_none());
}

#[test]
fn subtree_shadowing_overrides_ancestor_for_descendants_only() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct ColorTheme(&'static str);

    struct DynamicBranchComp;
    impl Component for DynamicBranchComp {
        type Props = ();
        type Message = ();
        fn create(_: &Self::Props, _: &mut ComponentContext<Self>) -> Self {
            Self
        }
        fn render(&self, _: &Self::Props, _: &RenderContext<Self>) -> UiNode {
            UiNode::empty()
        }
    }

    // Topology:
    //          Root (ColorTheme("base"))
    //         /                        \
    //   BranchA (no override)    BranchB (ColorTheme("override-dark"))
    //      |                            |
    //   LeafA                        LeafB

    let root_desc =
        ComponentDescriptor::of::<DynamicBranchComp>((), NodeKey::Explicit(Key::from("root")));
    let root_id = mount_node(&mut arena, None, 0, root_desc, &dirty_cell);
    arena.provide_context(root_id, ColorTheme("base"));

    let branch_a_desc =
        ComponentDescriptor::of::<DynamicBranchComp>((), NodeKey::Explicit(Key::from("branch_a")));
    let branch_a_id = mount_node(&mut arena, Some(root_id), 1, branch_a_desc, &dirty_cell);

    let leaf_a_desc =
        ComponentDescriptor::of::<DynamicBranchComp>((), NodeKey::Explicit(Key::from("leaf_a")));
    let leaf_a_id = mount_node(&mut arena, Some(branch_a_id), 2, leaf_a_desc, &dirty_cell);

    let branch_b_desc =
        ComponentDescriptor::of::<DynamicBranchComp>((), NodeKey::Explicit(Key::from("branch_b")));
    let branch_b_id = mount_node(&mut arena, Some(root_id), 1, branch_b_desc, &dirty_cell);
    // Subtree B overrides the ColorTheme:
    arena.provide_context(branch_b_id, ColorTheme("override-dark"));

    let leaf_b_desc =
        ComponentDescriptor::of::<DynamicBranchComp>((), NodeKey::Explicit(Key::from("leaf_b")));
    let leaf_b_id = mount_node(&mut arena, Some(branch_b_id), 2, leaf_b_desc, &dirty_cell);

    // LeafA should inherit from Root ("base")
    let ctx_a = ComponentContext::<DynamicBranchComp>::for_arena(leaf_a_id, &arena);
    assert_eq!(ctx_a.use_context::<ColorTheme>(), Some(ColorTheme("base")));

    // LeafB should receive BranchB's shadowed value ("override-dark")
    let ctx_b = ComponentContext::<DynamicBranchComp>::for_arena(leaf_b_id, &arena);
    assert_eq!(
        ctx_b.use_context::<ColorTheme>(),
        Some(ColorTheme("override-dark"))
    );

    // BranchA sees Root
    assert_eq!(
        arena.use_context::<ColorTheme>(branch_a_id),
        Some(ColorTheme("base"))
    );

    // BranchB sees its own override
    assert_eq!(
        arena.use_context::<ColorTheme>(branch_b_id),
        Some(ColorTheme("override-dark"))
    );
}

#[test]
fn context_update_and_rebind_support() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct CounterState(i32);

    struct NodeComp;
    impl Component for NodeComp {
        type Props = ();
        type Message = ();
        fn create(_: &Self::Props, _: &mut ComponentContext<Self>) -> Self {
            Self
        }
        fn render(&self, _: &Self::Props, _: &RenderContext<Self>) -> UiNode {
            UiNode::empty()
        }
    }

    struct OtherComp;

    let desc = ComponentDescriptor::of::<NodeComp>((), NodeKey::Explicit(Key::from("parent")));
    let parent_id = mount_node(&mut arena, None, 0, desc, &dirty_cell);

    let child_desc = ComponentDescriptor::of::<NodeComp>((), NodeKey::Explicit(Key::from("child")));
    let child_id = mount_node(&mut arena, Some(parent_id), 1, child_desc, &dirty_cell);

    // Provide initial context
    arena.provide_context(parent_id, CounterState(10));
    assert_eq!(
        arena.use_context::<CounterState>(child_id),
        Some(CounterState(10))
    );

    // Update context value on parent
    let parent_ctx = ComponentContext::<NodeComp>::with_arena(parent_id, &dirty_cell, &arena);
    parent_ctx.provide_context(CounterState(25));

    // Child sees updated value
    let child_ctx = ComponentContext::<NodeComp>::with_arena(child_id, &dirty_cell, &arena);
    assert_eq!(
        child_ctx.use_context::<CounterState>(),
        Some(CounterState(25))
    );

    // Rebound context retains arena and accesses context correctly
    let mut rebound_parent = parent_ctx;
    let rebound = rebound_parent.rebind::<OtherComp>();
    assert_eq!(
        rebound.use_context::<CounterState>(),
        Some(CounterState(25))
    );
}

#[test]
fn context_in_render_context_and_component_update() {
    let mut arena = Arena::new();
    let dirty_cell = Cell::new(false);

    #[derive(Clone, Debug, PartialEq)]
    struct SharedConfig {
        title: SharedStr,
    }

    struct ProviderComp;
    impl Component for ProviderComp {
        type Props = ();
        type Message = SharedStr;

        fn create(_: &Self::Props, ctx: &mut ComponentContext<Self>) -> Self {
            ctx.provide_context(SharedConfig {
                title: SharedStr::from_static("Initial"),
            });
            Self
        }

        fn update(&mut self, msg: Self::Message, ctx: &mut ComponentContext<Self>) -> ShouldRender {
            ctx.provide_context(SharedConfig { title: msg });
            ShouldRender::Yes
        }

        fn render(&self, _props: &Self::Props, ctx: &RenderContext<Self>) -> UiNode {
            let config = ctx
                .use_context::<SharedConfig>()
                .expect("config must exist");
            UiNode::text(config.title)
        }
    }

    let desc =
        ComponentDescriptor::of::<ProviderComp>((), NodeKey::Explicit(Key::from("provider")));
    let id = mount_node(&mut arena, None, 0, desc, &dirty_cell);

    // Initial render
    let node1 = arena.render(id, &dirty_cell).expect("render succeeds");
    match node1 {
        UiNode::Text(t) => assert_eq!(t.content.as_str(), "Initial"),
        _ => panic!("expected text node"),
    }

    // Dispatch update message to change context
    let _ = arena.update(
        id,
        Box::new(SharedStr::from_static("Updated Title")),
        &dirty_cell,
    );

    // Render after update
    let node2 = arena.render(id, &dirty_cell).expect("render succeeds");
    match node2 {
        UiNode::Text(t) => assert_eq!(t.content.as_str(), "Updated Title"),
        _ => panic!("expected text node"),
    }
}
