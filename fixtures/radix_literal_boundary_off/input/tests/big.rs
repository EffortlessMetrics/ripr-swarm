use radix_literal_boundary_off_fixture::is_big;

#[test]
fn small_is_not_big() {
    assert_eq!(is_big(0x10), false);
}
