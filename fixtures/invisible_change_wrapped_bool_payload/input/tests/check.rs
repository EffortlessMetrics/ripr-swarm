#[test]
fn five_is_checked() {
    assert!(matches!(invisible_change_wrapped_bool_payload::check(5), Some(true)));
}
