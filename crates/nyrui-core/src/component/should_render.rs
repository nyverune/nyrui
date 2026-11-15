//! Decision enum indicating whether a component requires re-rendering.

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

/// Indicates whether a component state mutation or property update requires re-rendering.
///
/// Returned by [`Component::update`](crate::component::Component::update) and
/// [`Component::update_props`](crate::component::Component::update_props) to signal
/// to the runtime reconciler whether a component's declarative subtree has changed.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Default)]
pub enum ShouldRender {
    /// The component state or properties changed, requiring a re-render pass.
    Yes,
    /// The component visual output is unchanged, allowing the reconciler to skip re-rendering.
    #[default]
    No,
}

impl ShouldRender {
    /// Returns `true` if this value is [`ShouldRender::Yes`].
    #[inline]
    #[must_use]
    pub const fn is_yes(self) -> bool {
        matches!(self, Self::Yes)
    }

    /// Returns `true` if this value is [`ShouldRender::No`].
    #[inline]
    #[must_use]
    pub const fn is_no(self) -> bool {
        matches!(self, Self::No)
    }

    /// Combines two rendering decisions with logical OR: returns `Yes` if either operand is `Yes`.
    #[inline]
    #[must_use]
    pub const fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::Yes, _) | (_, Self::Yes) => Self::Yes,
            (Self::No, Self::No) => Self::No,
        }
    }

    /// Combines two rendering decisions with logical AND: returns `Yes` only if both operands are `Yes`.
    #[inline]
    #[must_use]
    pub const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Yes, Self::Yes) => Self::Yes,
            _ => Self::No,
        }
    }
}

impl From<bool> for ShouldRender {
    #[inline]
    fn from(value: bool) -> Self {
        if value { Self::Yes } else { Self::No }
    }
}

impl From<ShouldRender> for bool {
    #[inline]
    fn from(value: ShouldRender) -> Self {
        value.is_yes()
    }
}

impl Not for ShouldRender {
    type Output = Self;

    #[inline]
    fn not(self) -> Self::Output {
        match self {
            Self::Yes => Self::No,
            Self::No => Self::Yes,
        }
    }
}

impl BitOr for ShouldRender {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        self.or(rhs)
    }
}

impl BitOrAssign for ShouldRender {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.or(rhs);
    }
}

impl BitAnd for ShouldRender {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self::Output {
        self.and(rhs)
    }
}

impl BitAndAssign for ShouldRender {
    #[inline]
    fn bitand_assign(&mut self, rhs: Self) {
        *self = self.and(rhs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_no() {
        assert_eq!(ShouldRender::default(), ShouldRender::No);
    }

    #[test]
    fn predicates() {
        assert!(ShouldRender::Yes.is_yes());
        assert!(!ShouldRender::Yes.is_no());
        assert!(ShouldRender::No.is_no());
        assert!(!ShouldRender::No.is_yes());
    }

    #[test]
    fn boolean_conversions() {
        assert_eq!(ShouldRender::from(true), ShouldRender::Yes);
        assert_eq!(ShouldRender::from(false), ShouldRender::No);
        assert!(bool::from(ShouldRender::Yes));
        assert!(!bool::from(ShouldRender::No));
    }

    #[test]
    fn logical_not() {
        assert_eq!(!ShouldRender::Yes, ShouldRender::No);
        assert_eq!(!ShouldRender::No, ShouldRender::Yes);
    }

    #[test]
    fn bitwise_or() {
        assert_eq!(ShouldRender::Yes | ShouldRender::Yes, ShouldRender::Yes);
        assert_eq!(ShouldRender::Yes | ShouldRender::No, ShouldRender::Yes);
        assert_eq!(ShouldRender::No | ShouldRender::Yes, ShouldRender::Yes);
        assert_eq!(ShouldRender::No | ShouldRender::No, ShouldRender::No);

        let mut val = ShouldRender::No;
        val |= ShouldRender::Yes;
        assert_eq!(val, ShouldRender::Yes);
    }

    #[test]
    fn bitwise_and() {
        assert_eq!(ShouldRender::Yes & ShouldRender::Yes, ShouldRender::Yes);
        assert_eq!(ShouldRender::Yes & ShouldRender::No, ShouldRender::No);
        assert_eq!(ShouldRender::No & ShouldRender::Yes, ShouldRender::No);
        assert_eq!(ShouldRender::No & ShouldRender::No, ShouldRender::No);

        let mut val = ShouldRender::Yes;
        val &= ShouldRender::No;
        assert_eq!(val, ShouldRender::No);
    }
}
