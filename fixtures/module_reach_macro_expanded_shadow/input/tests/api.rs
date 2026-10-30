#[macro_use]
mod common;
mk!();

#[test]
fn discount_boundary() {
    assert_eq!(qual_reach::inner::order_discount(10), 5);
    assert_eq!(qual_reach::inner::order_discount(9), 0);
}
