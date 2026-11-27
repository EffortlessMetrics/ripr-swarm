#[test]
fn rng_digit() {
    let mut rng = rg::Rng::new();
    assert_eq!(rng.digit(10), 2);
    assert_eq!(rng.digit(9), 1);
}
