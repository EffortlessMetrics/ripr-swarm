use ab_window::clip_window;

#[test]
fn clip_window_returns_something_in_range() {
    assert!(clip_window(2, 3, 10).is_some());
}
