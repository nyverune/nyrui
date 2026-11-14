use nyrui_core::id::NodeKey;
use nyrui_core::node::{Children, SlotFn, UiNode};
use nyrui_core::props::Props;

struct MockButton;

#[derive(Debug, PartialEq, Eq)]
struct ButtonProps {
    label: &'static str,
}

impl Props for ButtonProps {}

#[test]
fn nested_fragment_construction_and_traversal() {
    let tree = UiNode::fragment([
        UiNode::text("Header"),
        UiNode::component::<MockButton, _>(ButtonProps { label: "Click" }).with_key(10u64),
        UiNode::fragment([UiNode::text("Sub-item 1"), UiNode::text("Sub-item 2")]),
        UiNode::empty(),
    ]);

    if let UiNode::Fragment(children) = tree {
        assert_eq!(children.len(), 4);
        assert_eq!(children[0].key(), Some(&NodeKey::positional(0, 0)));
        assert_eq!(children[1].key(), Some(&NodeKey::explicit(10u64)));
        assert!(children[3].is_empty());
    } else {
        panic!("expected root Fragment");
    }
}

#[test]
fn branch_scoped_reconciliation_isolation() {
    let if_branch = UiNode::fragment([UiNode::component::<MockButton, _>(ButtonProps {
        label: "Save",
    })])
    .with_scope(0);

    let else_branch = UiNode::fragment([UiNode::component::<MockButton, _>(ButtonProps {
        label: "Save",
    })])
    .with_scope(1);

    if let (UiNode::Fragment(if_children), UiNode::Fragment(else_children)) =
        (if_branch, else_branch)
    {
        let if_key = if_children[0]
            .key()
            .expect("if branch child should have key");
        let else_key = else_children[0]
            .key()
            .expect("else branch child should have key");

        assert_ne!(if_key, else_key);
        assert_eq!(if_key, &NodeKey::positional(0, 0));
        assert_eq!(else_key, &NodeKey::positional(1, 0));
    } else {
        panic!("expected Fragments");
    }
}

#[test]
fn children_builder_and_iteration() {
    let mut children = Children::new();
    children.push(UiNode::text("First"));
    children.push(UiNode::text("Second"));

    assert_eq!(children.len(), 2);
    assert_eq!(children[0].key(), Some(&NodeKey::positional(0, 0)));

    let mut count = 0;
    for node in &children {
        assert!(!node.is_empty());
        count += 1;
    }
    assert_eq!(count, 2);

    let node: UiNode = children.into_node();
    match node {
        UiNode::Fragment(vec) => assert_eq!(vec.len(), 2),
        _ => panic!("expected Fragment from Children"),
    }
}

#[test]
fn scoped_slot_dynamic_list_rendering() {
    struct Project {
        id: u64,
        name: &'static str,
    }

    let projects = vec![
        Project {
            id: 1,
            name: "Nyrui",
        },
        Project {
            id: 2,
            name: "Nysh",
        },
    ];

    let row_slot = SlotFn::new(|proj: &Project| {
        UiNode::text(format!("[{}] {}", proj.id, proj.name)).with_key(proj.id)
    });

    let mut list_children = Children::with_capacity(projects.len());
    for proj in &projects {
        list_children.push(row_slot.render(proj));
    }

    assert_eq!(list_children.len(), 2);
    assert_eq!(list_children[0].key(), Some(&NodeKey::explicit(1u64)));
    assert_eq!(list_children[1].key(), Some(&NodeKey::explicit(2u64)));
}
