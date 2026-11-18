use rust_multi_site_helper_reach_fixture::parse;

#[test]
fn parses_a_plain_version() {
    assert!(parse("1.2.3").is_ok());
}
