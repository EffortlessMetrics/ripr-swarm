use qual_reach::order_discount;

#[test]
fn discount_boundary() {
    assert_eq!(order_discount(10), 10);
    assert_eq!(order_discount(9), 9);
}
