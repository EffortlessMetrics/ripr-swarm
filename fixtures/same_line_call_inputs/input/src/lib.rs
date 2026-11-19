pub fn shipping(weight: u32) -> u32 {
    if weight > 100 { 60 } else { 10 }
}

#[test]
fn shipping_boundary() {
    assert_eq!(shipping(101), 50); assert_eq!(shipping(100), 10);
}
