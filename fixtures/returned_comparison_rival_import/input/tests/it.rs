use rival::thr;
mod rival {
    pub fn thr(x: i32) -> bool { x > 0 }
}
mod owner_tests {
    use thr::thr;
    #[test]
    fn low() { assert!(!thr(50)); }
    #[test]
    fn high() { assert!(thr(500)); }
}
#[test]
fn at() { assert!(thr(101)); }

mod callable_shadow {
    use thr::thr;
    #[test]
    fn local() {
        let thr: fn(i32) -> bool = |x| x > 0;
        assert!(thr(101));
    }
}
