pub fn accepts(n: i32) -> bool {
    n > 2
}

#[test]
fn boundary() {
    assert!(!accepts(2));
    assert!(accepts(3));
}
