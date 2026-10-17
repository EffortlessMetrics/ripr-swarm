1. Changed behavior: tax_cents InvalidRate boundary `rate_bps > 10_000` -> `>= 10_000`; 100% rate now rejected.

2. Ideal discriminating test:
```rust
assert_eq!(tax_cents(100, 10_000), Ok(100));
```

3. Shape: free fn returning Result<i64, CustomError>; integer cents/bps
