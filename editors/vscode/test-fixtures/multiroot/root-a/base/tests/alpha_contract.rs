use root_alpha::alpha_discount;

#[test]
fn alpha_below_boundary() {
    assert_eq!(alpha_discount(50, 100), 50);
}

#[test]
fn alpha_far_above_boundary() {
    assert_eq!(alpha_discount(10_000, 100), 9_990);
}
