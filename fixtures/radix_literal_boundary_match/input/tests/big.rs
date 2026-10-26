use radix_literal_boundary_match_fixture::is_big;

#[test]
fn boundary_is_not_big() {
    assert_eq!(is_big(0xFF), false);
}
