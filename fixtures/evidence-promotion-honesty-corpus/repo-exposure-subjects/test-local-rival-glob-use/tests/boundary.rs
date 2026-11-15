mod local {
    pub fn discounted_total(_amount: i32, _threshold: i32) -> i32 { 777 }
}
use local::*;

#[test]
fn glob_rival() {
    assert_eq!(discounted_total(100, 100), 777);
}
