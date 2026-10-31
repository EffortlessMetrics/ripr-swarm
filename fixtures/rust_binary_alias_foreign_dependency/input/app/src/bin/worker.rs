use my_macros::wrapped_test as test;
fn value() -> u32 { 2 }
#[crate::test]
fn aliased() { assert_eq!(value(), 2); }
fn main() {}
