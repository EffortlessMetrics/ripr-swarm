pub fn price_with_tax(cents: u32) -> u32 {
    cents + cents / 10
}

#[cfg(any())]
#[test]
fn never_compiled() {
    assert_eq!(price_with_tax(100), 110);
}
