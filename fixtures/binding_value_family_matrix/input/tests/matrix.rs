use bv::{body_of, bump, gate, within};

#[test]
fn strip_prefix_boundary_is_observed() {
    assert_eq!(body_of("pre-fix"), "matched");
}

#[test]
fn strip_prefix_fallback_is_observed() {
    assert_eq!(body_of("raw"), "none");
}

#[test]
fn starts_with_boundary_is_observed() {
    assert!(gate("pre-fix", true));
}

#[test]
fn len_boundary_is_observed() {
    assert!(within("zz", "abc"));
}

#[test]
fn checked_add_boundary_is_observed() {
    assert!(bump(3));
}
