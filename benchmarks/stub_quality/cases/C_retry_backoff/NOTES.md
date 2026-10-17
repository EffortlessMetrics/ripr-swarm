1. Change: `backoff_ms` multiplier `2u64.pow(attempt)` -> `3u64.pow(attempt)` (src/lib.rs).
2. Ideal test: `assert_eq!(backoff_ms(3, 100), 800); assert_eq!(backoff_ms(1, 100), 200);`
3. Shape: free fn (also generic closure fn returning Result).
