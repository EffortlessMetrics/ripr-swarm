#[test]
fn low() {
    assert!(!thr::internal::thr(50));
}

#[test]
fn high() {
    assert!(thr::internal::thr(500));
}

#[test]
fn at() {
    assert!(!thr::thr(101));
}
