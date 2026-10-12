use match_arm_selected_credit_fixture::reason;

#[test]
fn none_arm_returns_zero() {
    assert_eq!(reason(None), 0);
}
