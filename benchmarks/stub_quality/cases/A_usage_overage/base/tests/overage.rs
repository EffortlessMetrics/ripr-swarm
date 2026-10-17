use usage_overage::overage_cents;

#[test]
fn bills_only_overage() {
    assert_eq!(overage_cents(&[100, 200], 250, 2), 100);
}

#[test]
fn nothing_when_under_included() {
    assert_eq!(overage_cents(&[10], 100, 5), 0);
    assert_eq!(overage_cents(&[], 0, 5), 0);
}
