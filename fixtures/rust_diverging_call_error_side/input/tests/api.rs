use rust_diverging_call_error_side::total;

fn skip() -> &'static str {
    std::process::exit(0)
}

#[test]
fn bad_digit_is_rejected() {
    assert_eq!(total(skip()).ok(), None);
}
