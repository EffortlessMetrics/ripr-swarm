mod helper {
    pub fn pick(n: u32) -> u32 {
        n + 1
    }
}

#[test]
fn picks_two() {
    assert_eq!(crate::helper::pick(1), 2);
}
