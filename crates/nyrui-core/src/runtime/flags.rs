//! State and lifecycle flags tracking node status in the arena.

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

/// Bitflags representing node lifecycle, focus, and scheduling state in the arena.
///
/// Implements the node flags defined in **RFC-0001 §6.2**:
/// - [`NodeFlags::DIRTY`]: Node has pending state or prop mutations and needs to be re-rendered.
/// - [`NodeFlags::MOUNTED`]: Node is currently attached to the active arena tree.
/// - [`NodeFlags::FOCUSED`]: Node currently holds keyboard focus.
/// - [`NodeFlags::FOCUSABLE`]: Node can participate in Tab / Shift+Tab keyboard focus traversal.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Default)]
pub struct NodeFlags(u8);

impl NodeFlags {
    /// Empty set of flags (0).
    pub const EMPTY: Self = Self(0);

    /// Flag indicating that this node needs to re-evaluate its [`Component::render`](crate::component::Component::render).
    pub const DIRTY: Self = Self(1 << 0);

    /// Flag indicating that this node is mounted in the active arena tree.
    pub const MOUNTED: Self = Self(1 << 1);

    /// Flag indicating that this node currently holds keyboard focus.
    pub const FOCUSED: Self = Self(1 << 2);

    /// Flag indicating that this node can receive keyboard focus.
    pub const FOCUSABLE: Self = Self(1 << 3);

    /// Returns an empty set of flags.
    #[inline]
    #[must_use]
    pub const fn empty() -> Self {
        Self::EMPTY
    }

    /// Returns `true` if all flags in `other` are contained within `self`.
    #[inline]
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Inserts the specified flags into the flag set.
    #[inline]
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Removes the specified flags from the flag set.
    #[inline]
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// Sets or clears the specified flags based on `value`.
    #[inline]
    pub fn set(&mut self, other: Self, value: bool) {
        if value {
            self.insert(other);
        } else {
            self.remove(other);
        }
    }

    /// Toggles the specified flags.
    #[inline]
    pub fn toggle(&mut self, other: Self) {
        self.0 ^= other.0;
    }

    /// Returns the underlying raw bits.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Creates a flag set from raw bits.
    #[inline]
    #[must_use]
    pub const fn from_bits_truncate(bits: u8) -> Self {
        Self(bits)
    }
}

impl BitOr for NodeFlags {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for NodeFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for NodeFlags {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for NodeFlags {
    #[inline]
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl Not for NodeFlags {
    type Output = Self;

    #[inline]
    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_operations() {
        let mut flags = NodeFlags::empty();
        assert!(!flags.contains(NodeFlags::DIRTY));
        assert!(!flags.contains(NodeFlags::MOUNTED));

        flags.insert(NodeFlags::DIRTY);
        assert!(flags.contains(NodeFlags::DIRTY));
        assert!(!flags.contains(NodeFlags::MOUNTED));

        flags.insert(NodeFlags::MOUNTED | NodeFlags::FOCUSABLE);
        assert!(flags.contains(NodeFlags::DIRTY));
        assert!(flags.contains(NodeFlags::MOUNTED));
        assert!(flags.contains(NodeFlags::FOCUSABLE));
        assert!(!flags.contains(NodeFlags::FOCUSED));

        flags.remove(NodeFlags::DIRTY);
        assert!(!flags.contains(NodeFlags::DIRTY));
        assert!(flags.contains(NodeFlags::MOUNTED));

        flags.set(NodeFlags::FOCUSED, true);
        assert!(flags.contains(NodeFlags::FOCUSED));
        flags.set(NodeFlags::FOCUSED, false);
        assert!(!flags.contains(NodeFlags::FOCUSED));

        flags.toggle(NodeFlags::MOUNTED);
        assert!(!flags.contains(NodeFlags::MOUNTED));
        flags.toggle(NodeFlags::MOUNTED);
        assert!(flags.contains(NodeFlags::MOUNTED));
    }

    #[test]
    fn bitwise_ops() {
        let combined = NodeFlags::DIRTY | NodeFlags::FOCUSED;
        assert_eq!(
            combined.bits(),
            NodeFlags::DIRTY.bits() | NodeFlags::FOCUSED.bits()
        );

        let masked = combined & NodeFlags::DIRTY;
        assert_eq!(masked, NodeFlags::DIRTY);

        let mut assigned = NodeFlags::empty();
        assigned |= NodeFlags::MOUNTED;
        assert_eq!(assigned, NodeFlags::MOUNTED);
        assigned &= NodeFlags::FOCUSABLE;
        assert_eq!(assigned, NodeFlags::empty());
    }
}
