use tuple_trap::Money;
use std::ops::Add;
#[test]
fn zero_does_not_distinguish() {
    let value = Money(0).add(Money(0));
    assert_eq!(value.0, 0);
}
