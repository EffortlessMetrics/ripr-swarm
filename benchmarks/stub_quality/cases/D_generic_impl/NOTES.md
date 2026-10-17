# generic_impl
1. Change (lib.rs): Stack::push full check `len >= cap` -> `len > cap`
2. Ideal test: `let mut s=Stack::with_capacity(1); s.push(1).unwrap(); assert_eq!(s.push(2), Err(2)); assert_eq!(s.len(),1)`
3. Shape: impl<T: Clone> block, private fields, no struct-literal construction (only with_capacity/Default); T must be chosen; Result<(),T> error carries the item.
