use rust_helper_expectation_literal_control::*;

fn expected() -> u32 {
    4
}

#[test]
fn quota_is_pinned() {
    assert_eq!(quota(2), expected());
}
