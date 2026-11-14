use guard_operand_dropped_agreement::is_pow2;

#[test]
fn powers_and_non_powers() {
    assert!(is_pow2(4));
    assert!(!is_pow2(6));
}
