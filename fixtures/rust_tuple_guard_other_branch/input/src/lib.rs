#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Money(pub i64);
pub fn f(a: Money, b: Money) -> Money {
    if a.0 > 100 {
        Money(b.0 + a.0)
    } else {
        Money(0)
    }
}

#[test]
fn observes() {
    assert_eq!(f(Money(2), Money(3)), Money(0));
}
