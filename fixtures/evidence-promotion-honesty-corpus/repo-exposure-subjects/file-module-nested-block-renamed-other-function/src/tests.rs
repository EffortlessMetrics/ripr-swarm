use super::*;

#[test]
fn nested_renamed_other() {
    {
        use super::other as discount;
        assert_eq!(discount(50), 5);
        assert_eq!(discount(49), 0);
    }
}
