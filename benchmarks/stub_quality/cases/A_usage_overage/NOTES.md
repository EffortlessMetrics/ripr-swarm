1. Changed behavior: MAX_BILLABLE_UNITS cap 10_000 -> 5_000 in overage_cents.

2. Ideal discriminating test:
```rust
assert_eq!(overage_cents(&[20_000], 0, 1), 10_000);
```

3. Shape: pub free fn taking &[u32]; covered only by tests/ integration dir; returns u64
