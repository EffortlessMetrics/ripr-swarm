use compile_error_build_time::limit;

#[test]
fn caps_at_eight() {
    assert_eq!(limit(12), 8);
}
