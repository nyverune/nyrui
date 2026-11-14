//! Node identity, stable reconciliation keys, and scoped positional keys.
//!
//! This module implements the identity primitives specified in **RFC-0001 §6.2 and §6.3**:
//! - [`NodeId`]: A generational identifier uniquely referencing a mounted component instance in the arena.
//! - [`Key`]: An explicit developer-assigned key used to preserve component state across reorderings.
//! - [`NodeKey`]: Internal reconciliation key distinguishing explicit keys from compile-time branch-scoped positional keys.

use crate::props::SharedStr;

use std::num::NonZeroU32;
use std::sync::Arc;

/// A generational identifier uniquely referencing a component instance in the runtime arena.
///
/// An instance's identity is defined by its arena slot `index` and its `generation`.
/// When a component is unmounted and its arena slot is recycled, the slot's generation
/// counter is incremented, guaranteeing that any stale references (e.g., from in-flight
/// asynchronous messages or lingering event callbacks) will not alias the new component.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct NodeId {
    /// Slot index of the component within the runtime instance arena.
    pub index: u32,
    /// Generational counter preventing stale references upon slot reuse.
    pub generation: NonZeroU32,
}

impl NodeId {
    /// Creates a new [`NodeId`] with the given slot index and generation.
    #[inline]
    #[must_use]
    pub const fn new(index: u32, generation: NonZeroU32) -> Self {
        Self { index, generation }
    }
}

/// An explicit developer-provided key identifying a component instance within dynamic structures.
///
/// Keys are used during reconciliation to preserve local component state when siblings
/// are reordered, inserted, or removed (RFC-0001 §6.3).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Key {
    /// Unsigned 64-bit integer identifier (e.g., database IDs, sequence numbers).
    U64(u64),
    /// Signed 64-bit integer identifier.
    I64(i64),
    /// Static string slice literal with zero heap allocations.
    StaticStr(&'static str),
    /// Cheaply cloned reference-counted string slice.
    SharedStr(Arc<str>),
}

impl From<u64> for Key {
    #[inline]
    fn from(value: u64) -> Self {
        Key::U64(value)
    }
}

impl From<usize> for Key {
    #[inline]
    fn from(value: usize) -> Self {
        Key::U64(value as u64)
    }
}

impl From<i64> for Key {
    #[inline]
    fn from(value: i64) -> Self {
        Key::I64(value)
    }
}

impl From<i32> for Key {
    #[inline]
    fn from(value: i32) -> Self {
        Key::I64(value as i64)
    }
}

impl From<&'static str> for Key {
    #[inline]
    fn from(value: &'static str) -> Self {
        Key::StaticStr(value)
    }
}

impl From<Arc<str>> for Key {
    #[inline]
    fn from(value: Arc<str>) -> Self {
        Key::SharedStr(value)
    }
}

impl From<String> for Key {
    #[inline]
    fn from(value: String) -> Self {
        Key::SharedStr(Arc::from(value))
    }
}

impl From<SharedStr> for Key {
    #[inline]
    fn from(value: SharedStr) -> Self {
        match value {
            SharedStr::Static(s) => Key::StaticStr(s),
            SharedStr::Shared(arc) => Key::SharedStr(arc),
        }
    }
}

/// Structural reconciliation key distinguishing explicit keys from scoped positional keys.
///
/// RFC-0001 §6.3 specifies that positional nodes in different syntactic branches
/// (such as `if` and `else` blocks inside `ui!`) must carry distinct compile-time `scope`
/// identifiers so that switching branches does not accidentally preserve or alias incompatible
/// state across distinct components.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum NodeKey {
    /// Explicit developer-provided key (`key: project.id`).
    Explicit(Key),
    /// Implicit positional key scoped to `(scope, index)`.
    Positional {
        /// Syntactic scope identifier assigned at compile time to conditional branches.
        scope: u16,
        /// Positional index of the child node within its scope.
        index: u32,
    },
}

impl NodeKey {
    /// Creates an explicit [`NodeKey`] wrapping a developer-assigned [`Key`].
    #[inline]
    #[must_use]
    pub fn explicit(key: impl Into<Key>) -> Self {
        NodeKey::Explicit(key.into())
    }

    /// Creates an implicit positional [`NodeKey`] with a branch scope and child index.
    #[inline]
    #[must_use]
    pub const fn positional(scope: u16, index: u32) -> Self {
        NodeKey::Positional { scope, index }
    }
}

impl From<Key> for NodeKey {
    #[inline]
    fn from(key: Key) -> Self {
        NodeKey::Explicit(key)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash_of<T: Hash>(x: &T) -> u64 {
        let mut h = DefaultHasher::new();
        x.hash(&mut h);
        h.finish()
    }

    fn make_generation(val: u32) -> NonZeroU32 {
        NonZeroU32::new(val).expect("test generation must be non-zero")
    }

    #[test]
    fn node_id_new_and_fields() {
        let id = NodeId::new(42, make_generation(1));
        assert_eq!(id.index, 42);
        assert_eq!(id.generation, make_generation(1));
    }

    #[test]
    fn node_id_eq_same_index_and_generation() {
        let a = NodeId::new(3, make_generation(1));
        let b = NodeId::new(3, make_generation(1));
        assert_eq!(a, b);
    }

    #[test]
    fn node_id_ne_different_generation() {
        let a = NodeId::new(3, make_generation(1));
        let b = NodeId::new(3, make_generation(2));
        assert_ne!(a, b);
    }

    #[test]
    fn node_id_ne_different_index() {
        let a = NodeId::new(1, make_generation(1));
        let b = NodeId::new(2, make_generation(1));
        assert_ne!(a, b);
    }

    #[test]
    fn node_id_hash_eq_consistency() {
        let a = NodeId::new(3, make_generation(1));
        let b = NodeId::new(3, make_generation(1));
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn node_id_hash_distinguishes_generation() {
        let a = NodeId::new(3, make_generation(1));
        let b = NodeId::new(3, make_generation(2));
        assert_ne!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn node_id_ordering() {
        let a = NodeId::new(1, make_generation(1));
        let b = NodeId::new(2, make_generation(1));
        let c = NodeId::new(2, make_generation(2));
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn node_id_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<NodeId>();
    }

    #[test]
    fn node_id_debug_format() {
        let id = NodeId::new(3, make_generation(1));
        let s = format!("{id:?}");
        assert!(s.contains("index: 3"));
        assert!(s.contains("generation: 1"));
    }

    #[test]
    fn node_id_option_uses_niche() {
        use std::mem::size_of;
        assert_eq!(size_of::<NodeId>(), 8);
        assert_eq!(size_of::<Option<NodeId>>(), 8);
    }

    #[test]
    fn key_eq_same_variant_same_value() {
        assert_eq!(Key::U64(42), Key::U64(42));
        assert_eq!(Key::I64(-7), Key::I64(-7));
        assert_eq!(Key::StaticStr("foo"), Key::StaticStr("foo"));
        assert_eq!(
            Key::SharedStr(Arc::from("bar")),
            Key::SharedStr(Arc::from("bar"))
        );
    }

    #[test]
    fn key_ne_same_variant_diff_value() {
        assert_ne!(Key::U64(1), Key::U64(2));
        assert_ne!(Key::I64(-1), Key::I64(1));
        assert_ne!(Key::StaticStr("a"), Key::StaticStr("b"));
        assert_ne!(
            Key::SharedStr(Arc::from("a")),
            Key::SharedStr(Arc::from("b"))
        );
    }

    #[test]
    fn key_variants_never_cross_equal() {
        assert_ne!(Key::U64(5), Key::I64(5));
        assert_ne!(Key::I64(5), Key::U64(5));

        assert_ne!(Key::StaticStr("x"), Key::SharedStr(Arc::from("x")));
        assert_ne!(Key::SharedStr(Arc::from("x")), Key::StaticStr("x"));

        assert_ne!(Key::U64(0), Key::StaticStr("0"));
        assert_ne!(Key::I64(0), Key::SharedStr(Arc::from("0")));
    }

    #[test]
    fn key_hash_eq_consistency() {
        assert_eq!(hash_of(&Key::U64(42)), hash_of(&Key::U64(42)));
        assert_eq!(hash_of(&Key::I64(-1)), hash_of(&Key::I64(-1)));
        assert_eq!(hash_of(&Key::StaticStr("s")), hash_of(&Key::StaticStr("s")));
        assert_eq!(
            hash_of(&Key::SharedStr(Arc::from("s"))),
            hash_of(&Key::SharedStr(Arc::from("s")))
        );
    }

    #[test]
    fn key_hash_distinguishes_variants() {
        assert_ne!(hash_of(&Key::U64(5)), hash_of(&Key::I64(5)));
        assert_ne!(
            hash_of(&Key::StaticStr("x")),
            hash_of(&Key::SharedStr(Arc::from("x")))
        );
    }

    #[test]
    fn key_works_as_hashmap_key() {
        use std::collections::HashMap;

        let mut map: HashMap<Key, &str> = HashMap::new();
        map.insert(Key::U64(1), "unsigned_one");
        map.insert(Key::I64(1), "signed_one");
        map.insert(Key::StaticStr("one"), "static_one");
        map.insert(Key::SharedStr(Arc::from("one")), "shared_one");

        assert_eq!(map.len(), 4);
        assert_eq!(map[&Key::U64(1)], "unsigned_one");
        assert_eq!(map[&Key::I64(1)], "signed_one");
        assert_eq!(map[&Key::StaticStr("one")], "static_one");
        assert_eq!(map[&Key::SharedStr(Arc::from("one"))], "shared_one");
    }

    #[test]
    fn key_from_u64() {
        assert_eq!(Key::from(42u64), Key::U64(42));
    }

    #[test]
    fn key_from_usize() {
        assert_eq!(Key::from(42usize), Key::U64(42));
    }

    #[test]
    fn key_from_i64() {
        assert_eq!(Key::from(-42i64), Key::I64(-42));
        assert_eq!(Key::from(42i64), Key::I64(42));
    }

    #[test]
    fn key_from_i32() {
        assert_eq!(Key::from(-42i32), Key::I64(-42));
        assert_eq!(Key::from(42i32), Key::I64(42));
    }

    #[test]
    fn key_from_static_str() {
        assert_eq!(Key::from("hello"), Key::StaticStr("hello"));
    }

    #[test]
    fn key_from_string() {
        let s = String::from("hello");
        assert_eq!(Key::from(s), Key::SharedStr(Arc::from("hello")));
    }

    #[test]
    fn key_from_arc_str() {
        let arc: Arc<str> = Arc::from("hello");
        assert_eq!(Key::from(arc.clone()), Key::SharedStr(arc));
    }

    #[test]
    fn key_from_u64_max() {
        assert_eq!(Key::from(u64::MAX), Key::U64(u64::MAX));
    }

    #[test]
    fn key_from_usize_to_u64_roundtrip() {
        let big = usize::MAX;
        match Key::from(big) {
            Key::U64(v) => assert_eq!(v as usize, big),
            _ => panic!("expected U64"),
        }
    }

    #[test]
    fn key_from_i32_min() {
        assert_eq!(Key::from(i32::MIN), Key::I64(i32::MIN as i64));
    }

    #[test]
    fn key_from_empty_string() {
        assert_eq!(Key::from(""), Key::StaticStr(""));
        assert_eq!(Key::from(String::new()), Key::SharedStr(Arc::from("")));
    }

    #[test]
    fn node_key_explicit_ne_positional() {
        let explicit = NodeKey::explicit(42u64);
        let positional = NodeKey::positional(0, 1);
        assert_ne!(explicit, positional);
        assert_ne!(positional, explicit);
    }

    #[test]
    fn node_key_positional_scope_distinguishes_branches() {
        let if_branch = NodeKey::positional(0, 0);
        let else_branch = NodeKey::positional(1, 0);

        assert_ne!(if_branch, else_branch);
        assert_ne!(hash_of(&if_branch), hash_of(&else_branch));
    }

    #[test]
    fn node_key_positional_index_distinguishes_siblings() {
        let first = NodeKey::positional(0, 0);
        let second = NodeKey::positional(0, 1);
        assert_ne!(first, second);
    }

    #[test]
    fn node_key_explicit_eq_same_key() {
        assert_eq!(NodeKey::explicit(42u64), NodeKey::explicit(42u64));
    }

    #[test]
    fn node_key_explicit_ne_diff_key() {
        assert_ne!(NodeKey::explicit(42u64), NodeKey::explicit(43u64));
    }

    #[test]
    fn node_key_explicit_ne_diff_variant() {
        assert_ne!(NodeKey::explicit(42u64), NodeKey::explicit(42i64));
    }

    #[test]
    fn node_key_from_key() {
        let key = Key::U64(10);
        let node_key: NodeKey = key.clone().into();
        assert_eq!(node_key, NodeKey::Explicit(key));
    }

    #[test]
    fn node_key_hash_eq_consistency() {
        let a = NodeKey::explicit(1u64);
        let b = NodeKey::explicit(1u64);
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));

        let c = NodeKey::positional(0, 0);
        let d = NodeKey::positional(0, 0);
        assert_eq!(c, d);
        assert_eq!(hash_of(&c), hash_of(&d));
    }

    #[test]
    fn node_key_works_as_hashset_member() {
        use std::collections::HashSet;

        let mut set: HashSet<NodeKey> = HashSet::new();
        set.insert(NodeKey::explicit(1u64));
        set.insert(NodeKey::explicit(1u64));
        set.insert(NodeKey::positional(0, 0));
        set.insert(NodeKey::positional(1, 0));
        set.insert(NodeKey::positional(0, 1));

        assert_eq!(set.len(), 4);
    }

    #[test]
    fn rfc_scenario_if_else_textinput_no_alias() {
        let editing_branch = NodeKey::positional(0, 0);
        let viewing_branch = NodeKey::positional(1, 0);

        assert_ne!(editing_branch, viewing_branch);
    }
}
