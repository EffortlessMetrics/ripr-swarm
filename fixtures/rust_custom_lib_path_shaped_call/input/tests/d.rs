#[test]
fn discounts() {
    assert_eq!(my_pkg::core::pricing::discount(100), 100);
    assert_eq!(my_pkg::core::pricing::discount(101), 91);
}
