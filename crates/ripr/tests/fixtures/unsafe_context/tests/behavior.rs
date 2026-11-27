use unsafe_context::accepts;

#[test]
fn checks_boundary() {
    assert_eq!(accepts(2), false);
    assert_eq!(accepts(3), true);
}
