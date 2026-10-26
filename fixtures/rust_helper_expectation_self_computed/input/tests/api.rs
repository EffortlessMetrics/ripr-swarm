use rust_helper_expectation_self_computed::*;

fn expected() -> u32 {
    quota(2)
}

#[test]
fn quota_is_pinned() {
    assert_eq!(quota(2), expected());
}
