use invisible_change_relational_sibling::backoff_ms;

#[test]
fn first_attempt_waits_the_base() {
    assert_eq!(backoff_ms(0, 100), 100);
}

#[test]
fn retries_grow_past_the_first_wait() {
    assert!(backoff_ms(3, 100) > backoff_ms(1, 100));
}
