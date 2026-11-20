use multi_line_dropped_conjunct_fixture::tier;

#[test]
fn one_hundred_is_tier_zero() {
    assert_eq!(tier(100, 0), 0);
}
