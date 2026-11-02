use compile_time_assert_stop_reason::n;

#[test]
fn n_is_twelve() {
    assert_eq!(n(), 12);
}
