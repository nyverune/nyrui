//! Component properties and zero-cost string sharing.
//!
//! Provides the [`Props`] trait and [`SharedStr`] container as specified in **RFC-0001 §5.2**.

pub mod shared_str;

pub use shared_str::SharedStr;

/// Marker and lifecycle contract for component properties.
///
/// In Nyrui, all components receive properties conforming to the [`Props`] trait
/// as specified in **RFC-0001 §5.2**.
///
/// Properties must satisfy `'static` so they can be retained inside the component arena
/// across frame renders.
///
/// # Fast Equality Hook (`props_eq`)
///
/// To optimize performance during reconciliation (RFC-0001 §6.3), the runtime queries
/// [`Props::props_eq`]. If two successive property instances are equal, the reconciler
/// can skip re-evaluating the component's `render` method and keep the existing subtree intact.
///
/// By default, `props_eq` returns `false` (pessimistic update). Implementations or derived
/// property structs whose fields implement [`PartialEq`] can override this method to return
/// `self == other`.
pub trait Props: 'static {
    /// Compares two property instances for equality.
    ///
    /// Returns `true` if `self` and `other` are guaranteed to be equivalent, allowing
    /// the runtime reconciler to bypass subtree re-rendering.
    ///
    /// The default implementation returns `false`.
    #[inline]
    fn props_eq(&self, other: &Self) -> bool {
        let _ = other;
        false
    }
}

/// Blanket implementation for stateless or empty components using the unit type `()`.
///
/// Because empty properties carry no state or parameters, successive instances of `()`
/// are always identical. Therefore, `props_eq` unconditionally returns `true`.
impl Props for () {
    #[inline]
    fn props_eq(&self, _other: &Self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct CustomProps {
        pub title: &'static str,
        pub count: u32,
    }

    impl Props for CustomProps {
        fn props_eq(&self, other: &Self) -> bool {
            self == other
        }
    }

    #[derive(Debug)]
    struct DefaultProps {
        pub value: u32,
    }

    impl Props for DefaultProps {}

    #[test]
    fn unit_props_always_equal() {
        assert!(().props_eq(&()));
    }

    #[test]
    fn default_props_eq_returns_false() {
        let p1 = DefaultProps { value: 10 };
        let p2 = DefaultProps { value: 10 };
        assert_eq!(p1.value, p2.value);
        assert!(!p1.props_eq(&p2));
    }

    #[test]
    fn custom_props_eq_reflects_equality() {
        let p1 = CustomProps {
            title: "Dashboard",
            count: 1,
        };
        let p2 = CustomProps {
            title: "Dashboard",
            count: 1,
        };
        let p3 = CustomProps {
            title: "Settings",
            count: 2,
        };

        assert!(p1.props_eq(&p2));
        assert!(!p1.props_eq(&p3));
    }
}
