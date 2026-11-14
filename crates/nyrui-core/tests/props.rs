use nyrui_core::props::{Props, SharedStr};
use std::sync::Arc;

#[test]
fn module_paths_and_reexports_are_consistent() {
    let s1: SharedStr = "hello".into();
    let s2: SharedStr = s1.clone();
    assert_eq!(s1, s2);

    fn assert_props<P: Props>() {}
    assert_props::<()>();
}

#[test]
fn shared_str_equality_across_static_and_allocated() {
    let static_str = SharedStr::Static("nyrui");
    let shared_arc = SharedStr::Shared(Arc::from("nyrui"));
    let from_string = SharedStr::from(String::from("nyrui"));

    assert_eq!(static_str, shared_arc);
    assert_eq!(shared_arc, from_string);
    assert_eq!(static_str, from_string);
    assert_eq!(static_str, "nyrui");
    assert_eq!("nyrui", static_str);
}

#[test]
fn shared_str_o1_clone_shares_pointer() {
    let big_data = "a".repeat(50_000);
    let original = SharedStr::from(big_data);
    let cloned = original.clone();

    match (&original, &cloned) {
        (SharedStr::Shared(a), SharedStr::Shared(b)) => {
            assert!(Arc::ptr_eq(a, b));
        }
        _ => panic!("expected Shared variant"),
    }
}

#[test]
fn unit_props_equality_contract() {
    let u1 = ();
    let u2 = ();
    assert!(u1.props_eq(&u2));
}

#[derive(Debug, PartialEq, Eq)]
struct ButtonProps {
    pub label: SharedStr,
    pub disabled: bool,
}

impl Props for ButtonProps {
    fn props_eq(&self, other: &Self) -> bool {
        self == other
    }
}

#[test]
fn custom_props_equality_with_shared_str() {
    let p1 = ButtonProps {
        label: "Submit".into(),
        disabled: false,
    };
    let p2 = ButtonProps {
        label: SharedStr::from(String::from("Submit")),
        disabled: false,
    };
    let p3 = ButtonProps {
        label: "Submit".into(),
        disabled: true,
    };

    assert!(p1.props_eq(&p2));
    assert!(!p1.props_eq(&p3));
}
