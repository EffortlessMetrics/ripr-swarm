use predicate_pairing_boundary_precision_residuals_7042::gate;

#[test]
fn after_assert() {
    let mut got = gate(10);
    assert_eq!(got, true);
    got = false;
}
