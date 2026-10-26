use rust_const_expectation_self_computed::*;

const EXPECTED: u32 = quota(2);

#[test]
fn quota_is_pinned() {
    assert_eq!(quota(2), EXPECTED);
}
