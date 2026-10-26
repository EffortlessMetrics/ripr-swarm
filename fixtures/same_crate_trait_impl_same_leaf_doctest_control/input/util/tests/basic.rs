#[test]
fn runs() {
    assert_eq!(rutil::block_on(|| 1 + 1), 2);
}
