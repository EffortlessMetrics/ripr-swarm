#[test]
fn delimiter_boundary_returns_prefix() {
    assert_eq!(split_after("a.b", '.'), "a");
}
