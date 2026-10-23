use std as qual_reach;
use ::qual_reach::inner::order_discount;

#[test]
fn discount_boundary() {
    assert_eq!(order_discount(10), 5);
    assert_eq!(order_discount(9), 0);
}
