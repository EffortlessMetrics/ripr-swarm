proptest::proptest! {
    #[test]
    fn exact_boundary(amount in proptest::strategy::Just(100i32)) {
        proptest::prop_assert_eq!(lib_proptest::discounted_total(amount, 100), 90);
    }
}
