use rust_const_expectation_literal_control::*;

const EXPECTED: u32 = 4;

#[test]
fn quota_is_pinned() {
    assert_eq!(quota(2), EXPECTED);
}
