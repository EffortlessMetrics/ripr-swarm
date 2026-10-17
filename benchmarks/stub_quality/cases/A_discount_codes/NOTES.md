1. Changed behavior: MAX_PCT cap constant 50 -> 60 in parse_discount.

2. Ideal discriminating test:
```rust
assert_eq!(parse_discount("SAVE55"), Some(50));
```

3. Shape: free fn taking &str; returns Option<u32>; private const
