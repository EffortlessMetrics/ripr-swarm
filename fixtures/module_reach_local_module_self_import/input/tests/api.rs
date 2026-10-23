pub mod qual_reach {
    pub mod inner {
        pub fn order_discount(_: u32) -> u32 { 5 }
    }
}
use self::qual_reach::inner::order_discount;
#[test]
fn discount_boundary() {
    assert_eq!(order_discount(10), 5);
    assert_eq!(order_discount(9), 0);
}
