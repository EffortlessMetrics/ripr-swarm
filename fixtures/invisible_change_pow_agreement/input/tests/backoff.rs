use invisible_change_pow_agreement::backoff_ms;

#[test]
fn first_attempt_waits_the_base() {
    assert_eq!(backoff_ms(0, 100), 100);
}
