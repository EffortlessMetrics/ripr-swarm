use discount_threshold::discounted_total;

#[test]
fn below_threshold_is_unchanged() {
    assert_eq!(discounted_total(9_999), 9_999);
}

#[test]
fn above_threshold_is_discounted() {
    assert_eq!(discounted_total(10_001), 9_501);
}

#[test]
fn exactly_at_threshold_is_discounted() {
    assert_eq!(discounted_total(10_000), 9_500);
}
