1. Changed behavior: half_up rounding offset `(v + 5)/10` -> `(v + 4)/10`; ties now round down.

2. Ideal discriminating test:
```rust
assert_eq!(round_to_cents(1235), 124);
```

3. Shape: private fn in nested mod (internal::half_up), reached via pub fn wrapper; integer rounding
