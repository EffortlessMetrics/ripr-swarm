# async_fn
1. Change (lib.rs): discount_pct `qty >= 10` -> `qty > 10`
2. Ideal test: `assert_eq!(block_on(discount_pct(10)), 10)` (head 0)
3. Shape: async fn needs an executor; no runtime dep, so generator must discover/hand-roll block_on already in crate.
