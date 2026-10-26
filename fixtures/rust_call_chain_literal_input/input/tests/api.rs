use rust_call_chain_literal_input::is_adult;

fn age_from_env(default: u32) -> u32 {
    std::env::var("AGE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[test]
fn adults_are_adults() {
    assert_eq!(is_adult(age_from_env(18)), true);
    assert_eq!(is_adult(age_from_env(30)), true);
}
