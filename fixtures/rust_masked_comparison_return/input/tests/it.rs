use thr::thr;
#[test]
fn t() {
    assert_eq!(thr("alpha", false), false);
    assert_eq!(thr("beta", false), false);
}
