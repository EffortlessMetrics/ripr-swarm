1. Changed behavior: bulk discount multiplier 0.90 -> 0.85 for qty >= 10.

2. Ideal discriminating test:
```rust
assert!((bulk_price(10.0, 10) - 90.0).abs() < 1e-9);
```

3. Shape: free fn with f64 args/return; constant in branch
