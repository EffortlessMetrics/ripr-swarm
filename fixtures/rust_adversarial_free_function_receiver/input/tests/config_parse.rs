use rust_adversarial_free_function_receiver_fixture::Config;

#[test]
fn config_strict_parse_reports_length() {
    let config = Config { strict: true };
    let input = "hey";
    assert_eq!(config.parse(input).1, Some(3));
}
