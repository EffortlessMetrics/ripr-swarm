mod a {
    pub fn gate(input: u32) -> bool {
        input >= 10
    }
}
mod b {
    pub fn gate(input: u32) -> bool {
        input >= 10
    }
}
#[test]
fn same_test_separate_owners() {
    let _ = a::gate(10);
    assert_eq!(a::gate(11), true);
    assert_eq!(b::gate(10), true);
}
