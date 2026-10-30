use qual_reach::inner::order_discount;

#[test]
fn discount_boundary() {
    use foreign::order_discount;
    assert_eq!(order_discount(10), 5);
    assert_eq!(order_discount(9), 0);
}
