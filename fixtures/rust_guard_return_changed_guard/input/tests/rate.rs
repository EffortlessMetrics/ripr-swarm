use guard_return_changed_guard_fixture::rate;

#[test]
fn three_hundred_rates_sixty() {
    assert_eq!(rate(300), 60);
}
