//! Shared string representation for zero-allocation static literals and O(1) cloning.

use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;

/// A lightweight, cheaply clonable string container.
///
/// `SharedStr` optimizes component property passing in Nyrui as specified in **RFC-0001 §5.2**:
/// - [`SharedStr::Static`]: Holds compile-time `&'static str` literals with zero heap allocations and $O(1)$ copies.
/// - [`SharedStr::Shared`]: Holds runtime strings inside an [`Arc<str>`], making prop cloning during reconciliation $O(1)$ without deep buffer reallocations.
///
/// Implements [`Deref<Target = str>`](std::ops::Deref), [`AsRef<str>`], [`Display`](fmt::Display),
/// [`PartialEq`], [`Eq`], [`Hash`], [`Default`], and various [`From`] conversions.
#[derive(Debug)]
pub enum SharedStr {
    /// A static string slice literal with zero heap allocations.
    Static(&'static str),
    /// A reference-counted string slice shared via [`Arc<str>`].
    Shared(Arc<str>),
}

impl SharedStr {
    /// Creates a [`SharedStr`] from a static string slice literal.
    #[inline]
    #[must_use]
    pub const fn from_static(s: &'static str) -> Self {
        SharedStr::Static(s)
    }

    /// Creates a [`SharedStr`] from an existing [`Arc<str>`].
    #[inline]
    #[must_use]
    pub const fn from_shared(arc: Arc<str>) -> Self {
        SharedStr::Shared(arc)
    }

    /// Returns a borrowed `&str` slice of the underlying string data.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            SharedStr::Static(s) => s,
            SharedStr::Shared(arc) => arc,
        }
    }

    /// Returns `true` if the string contains no bytes.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_str().is_empty()
    }

    /// Returns the length of the string in bytes.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_str().len()
    }

    /// Returns `true` if this instance holds a static string literal.
    #[inline]
    #[must_use]
    pub const fn is_static(&self) -> bool {
        matches!(self, SharedStr::Static(_))
    }

    /// Returns `true` if this instance holds a reference-counted shared string.
    #[inline]
    #[must_use]
    pub const fn is_shared(&self) -> bool {
        matches!(self, SharedStr::Shared(_))
    }
}

impl Clone for SharedStr {
    #[inline]
    fn clone(&self) -> Self {
        match self {
            SharedStr::Static(s) => SharedStr::Static(s),
            SharedStr::Shared(arc) => SharedStr::Shared(Arc::clone(arc)),
        }
    }
}

impl Deref for SharedStr {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for SharedStr {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::borrow::Borrow<str> for SharedStr {
    #[inline]
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for SharedStr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq for SharedStr {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for SharedStr {}

impl PartialOrd for SharedStr {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SharedStr {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for SharedStr {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl Default for SharedStr {
    #[inline]
    fn default() -> Self {
        SharedStr::Static("")
    }
}

impl From<&'static str> for SharedStr {
    #[inline]
    fn from(s: &'static str) -> Self {
        SharedStr::Static(s)
    }
}

impl From<String> for SharedStr {
    #[inline]
    fn from(s: String) -> Self {
        SharedStr::Shared(Arc::from(s))
    }
}

impl From<&String> for SharedStr {
    #[inline]
    fn from(s: &String) -> Self {
        SharedStr::Shared(Arc::from(s.as_str()))
    }
}

impl From<Arc<str>> for SharedStr {
    #[inline]
    fn from(s: Arc<str>) -> Self {
        SharedStr::Shared(s)
    }
}

impl From<Box<str>> for SharedStr {
    #[inline]
    fn from(s: Box<str>) -> Self {
        SharedStr::Shared(Arc::from(s))
    }
}

impl From<Cow<'static, str>> for SharedStr {
    #[inline]
    fn from(s: Cow<'static, str>) -> Self {
        match s {
            Cow::Borrowed(b) => SharedStr::Static(b),
            Cow::Owned(o) => SharedStr::Shared(Arc::from(o)),
        }
    }
}

impl PartialEq<str> for SharedStr {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for SharedStr {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for SharedStr {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<SharedStr> for str {
    #[inline]
    fn eq(&self, other: &SharedStr) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<SharedStr> for &str {
    #[inline]
    fn eq(&self, other: &SharedStr) -> bool {
        *self == other.as_str()
    }
}

impl PartialEq<SharedStr> for String {
    #[inline]
    fn eq(&self, other: &SharedStr) -> bool {
        self.as_str() == other.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Borrow;

    #[test]
    fn eq_same_variant() {
        assert_eq!(SharedStr::Static("a"), SharedStr::Static("a"));
        assert_eq!(
            SharedStr::Shared(Arc::from("a")),
            SharedStr::Shared(Arc::from("a"))
        );
    }

    #[test]
    fn eq_across_variants() {
        assert_eq!(SharedStr::Static("a"), SharedStr::Shared(Arc::from("a")));
        assert_eq!(SharedStr::Shared(Arc::from("a")), SharedStr::Static("a"));
    }

    #[test]
    fn ne_diff_content() {
        assert_ne!(SharedStr::Static("a"), SharedStr::Static("b"));
        assert_ne!(SharedStr::Static("a"), SharedStr::Shared(Arc::from("b")));
    }

    #[test]
    fn eq_with_primitive_strings() {
        let s = SharedStr::from("test_content");
        assert_eq!(s, "test_content");
        assert_eq!("test_content", s);
        assert_eq!(s, String::from("test_content"));
        assert_eq!(String::from("test_content"), s);
    }

    #[test]
    fn ordering_and_comparison() {
        let a = SharedStr::from("alpha");
        let b = SharedStr::from("beta");
        assert!(a < b);
        assert!(b > a);
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Less);
    }

    #[test]
    fn hash_consistent_with_eq() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_of<T: Hash>(x: &T) -> u64 {
            let mut h = DefaultHasher::new();
            x.hash(&mut h);
            h.finish()
        }

        assert_eq!(
            hash_of(&SharedStr::Static("x")),
            hash_of(&SharedStr::Shared(Arc::from("x")))
        );
        assert_ne!(
            hash_of(&SharedStr::Static("x")),
            hash_of(&SharedStr::Static("y"))
        );
    }

    #[test]
    fn clone_is_o1_and_shares_buffer() {
        let large_string = "x".repeat(1_000_000);
        let original = SharedStr::Shared(Arc::from(large_string));
        let cloned = original.clone();

        match (&original, &cloned) {
            (SharedStr::Shared(a), SharedStr::Shared(b)) => {
                assert!(Arc::ptr_eq(a, b));
            }
            _ => panic!("expected Shared variant"),
        }
    }

    #[test]
    fn static_clone_preserves_static_variant() {
        let original = SharedStr::from_static("static_literal");
        let cloned = original.clone();
        assert!(original.is_static());
        assert!(cloned.is_static());
        assert_eq!(original, cloned);
    }

    #[test]
    fn default_is_empty() {
        assert_eq!(SharedStr::default(), SharedStr::Static(""));
        assert!(SharedStr::default().is_empty());
        assert_eq!(SharedStr::default().len(), 0);
    }

    #[test]
    fn deref_and_borrow_support() {
        let s = SharedStr::from("deref_test");
        assert_eq!(s.len(), 10);
        assert_eq!(&s[0..5], "deref");
        assert_eq!(s.as_str(), "deref_test");
        let borrowed: &str = s.borrow();
        assert_eq!(borrowed, "deref_test");
    }

    #[test]
    fn display_formatting() {
        let s = SharedStr::from("display_test");
        assert_eq!(format!("{s}"), "display_test");
    }

    #[test]
    fn from_conversions() {
        assert_eq!(SharedStr::from("a"), SharedStr::Static("a"));
        assert_eq!(
            SharedStr::from(String::from("a")),
            SharedStr::Shared(Arc::from("a"))
        );
        assert_eq!(
            SharedStr::from(&String::from("a")),
            SharedStr::Shared(Arc::from("a"))
        );
        assert_eq!(
            SharedStr::from(Arc::from("a")),
            SharedStr::Shared(Arc::from("a"))
        );
        assert_eq!(
            SharedStr::from(Box::from("a")),
            SharedStr::Shared(Arc::from("a"))
        );
        assert_eq!(SharedStr::from(Cow::Borrowed("a")), SharedStr::Static("a"));
        assert_eq!(
            SharedStr::from(Cow::Owned(String::from("a"))),
            SharedStr::Shared(Arc::from("a"))
        );
    }

    #[test]
    fn constructors_and_inspectors() {
        let s1 = SharedStr::from_static("literal");
        assert!(s1.is_static());
        assert!(!s1.is_shared());

        let s2 = SharedStr::from_shared(Arc::from("shared"));
        assert!(s2.is_shared());
        assert!(!s2.is_static());
    }
}
