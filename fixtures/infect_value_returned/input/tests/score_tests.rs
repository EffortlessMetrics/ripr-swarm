use infect_value_returned_fixture::score;

#[test]
fn score_returns_expected_value() {
    assert_eq!(score(10), 10);
}
