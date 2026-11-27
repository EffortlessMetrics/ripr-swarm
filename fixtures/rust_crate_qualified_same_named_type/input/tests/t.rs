#[test]
fn rng_digit() {
    let mut rng = rust_crate_qualified_same_named_type_fixture::Rng::new();
    assert_eq!(rng.digit(10), 2);
    assert_eq!(rng.digit(9), 1);
}
