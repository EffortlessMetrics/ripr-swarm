1. Changed behavior: Plan::Enterprise per-seat price 4900 -> 4500 in match arm.

2. Ideal discriminating test:
```rust
assert_eq!(seat_price_cents(Plan::Enterprise, 2), 9800);
```

3. Shape: pub const fn; match on enum; returns u32
