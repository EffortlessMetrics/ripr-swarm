pub mod qual_reach {
    pub mod inner {
        pub use std::cmp::max as order_discount;
    }
}
use self::qual_reach::inner::order_discount;
#[test]
fn discount_boundary() {
    assert_eq!(order_discount(10), 5);
    assert_eq!(order_discount(9), 0);
}
