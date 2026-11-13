use nyrui_core::id::{Key, NodeId, NodeKey};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;

fn make_generation(val: u32) -> NonZeroU32 {
    NonZeroU32::new(val).expect("generation must be non-zero in test")
}

#[test]
fn node_id_works_as_arena_key() {
    let mut arena: HashMap<NodeId, &str> = HashMap::new();
    let id1 = NodeId::new(0, make_generation(1));
    let id2 = NodeId::new(0, make_generation(2));

    arena.insert(id1, "old_instance");
    arena.insert(id2, "new_instance");

    assert_eq!(arena.len(), 2);
    assert_eq!(arena.get(&id1), Some(&"old_instance"));
    assert_eq!(arena.get(&id2), Some(&"new_instance"));
}

#[test]
fn node_id_generation_invalidation() {
    let old_id = NodeId::new(5, make_generation(1));
    let new_id = NodeId::new(5, make_generation(2));

    assert_ne!(old_id, new_id);
}

#[test]
fn key_conversions_integration() {
    let u64_key: Key = 100u64.into();
    let usize_key: Key = 100usize.into();
    let i64_key: Key = (-100i64).into();
    let i32_key: Key = (-100i32).into();
    let static_str_key: Key = "static_slice".into();
    let string_key: Key = String::from("owned_string").into();

    assert_eq!(u64_key, Key::U64(100));
    assert_eq!(usize_key, Key::U64(100));
    assert_eq!(i64_key, Key::I64(-100));
    assert_eq!(i32_key, Key::I64(-100));
    assert_eq!(static_str_key, Key::StaticStr("static_slice"));
    assert_eq!(string_key, Key::SharedStr("owned_string".into()));
}

#[test]
fn node_key_branch_isolation_integration() {
    let mut active_nodes = HashSet::new();

    let if_branch_input = NodeKey::positional(0, 0);
    let else_branch_input = NodeKey::positional(1, 0);

    active_nodes.insert(if_branch_input.clone());
    assert!(!active_nodes.contains(&else_branch_input));

    active_nodes.insert(else_branch_input);
    assert_eq!(active_nodes.len(), 2);
}
