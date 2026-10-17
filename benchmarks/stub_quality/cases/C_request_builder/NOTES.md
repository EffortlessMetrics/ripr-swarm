1. Change: `build()` default `timeout_ms.unwrap_or(30_000)` -> `unwrap_or(3_000)` (src/lib.rs struct literal field).
2. Ideal test: `assert_eq!(RequestBuilder::new("u").build().timeout_ms, 30_000);`
3. Shape: builder chain, struct literal field, needs-constructor-args (url), consumes self.
