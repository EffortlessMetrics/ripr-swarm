use super::order_discount;

#[test]
fn discount_boundary() {
    fn order_discount(qty: u32) -> u32 {
        qty
    }
    assert_eq!(order_discount(10), 5);
    assert_eq!(order_discount(9), 0);
}
