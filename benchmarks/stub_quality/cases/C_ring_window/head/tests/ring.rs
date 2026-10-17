use ring_window::window::ring::Ring;
#[test] fn starts_empty() { let r: Ring<i32> = Ring::new(3); assert!(r.is_empty()); }
#[test] fn push_below_cap() { let mut r = Ring::new(3); assert_eq!(r.push(1), None); assert_eq!(r.push(2), None); assert_eq!(r.len(), 2); }
#[test] fn order_kept() { let mut r = Ring::new(5); r.push(1); r.push(2); assert_eq!(r.as_slice(), &[1, 2]); }
