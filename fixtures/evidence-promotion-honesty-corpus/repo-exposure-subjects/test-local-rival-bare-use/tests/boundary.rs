mod local {
    pub fn discounted_total(_amount: i32, _threshold: i32) -> i32 { 777 }
}
use local::discounted_total;

#[test]
fn bare_rival() {
    assert_eq!(discounted_total(100, 100), 777);
}
