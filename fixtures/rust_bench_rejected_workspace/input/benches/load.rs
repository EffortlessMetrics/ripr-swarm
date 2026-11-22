#[test]
fn fee_doubles() {
    assert_eq!(billing::fee(4), 8);
}
