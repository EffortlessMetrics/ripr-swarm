use crate::used::discount;

#[test]
fn discount_applies() {
    assert_eq!(discount(100), 100);
}
