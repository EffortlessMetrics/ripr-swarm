use super::*;

#[test]
fn nested_rename_then_owner() {
    {
        use super::other as discount;
        assert_eq!(discount(50), 5);
    }
    assert_eq!(discount(100), 10);
    assert_eq!(discount(99), 0);
}
