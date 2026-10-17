1. Changed behavior: TokenBucket::allow(&mut self) boundary `tokens >= n` -> `tokens > n`; exact-capacity request now denied.

2. Ideal discriminating test:
```rust
let mut b = TokenBucket::new(5); assert!(b.allow(5));
```

3. Shape: &mut self method on struct with new(); returns bool
