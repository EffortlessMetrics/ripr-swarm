#[test]
fn a_zero_fee_is_zero() {
    assert_eq!(invisible_change_ok_wrapped_owner_call::fee(0), Ok(0));
}

#[test]
fn a_zero_fee_matches_zero() {
    assert!(matches!(invisible_change_ok_wrapped_owner_call::fee(0), Ok(0)));
}
