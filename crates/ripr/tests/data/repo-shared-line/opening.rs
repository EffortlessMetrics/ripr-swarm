pub fn price(n: i32) -> i32 { if n > 10 { n - 1 } else { n } } #[test] fn check_price() { assert_eq!(price(11), 10); }
