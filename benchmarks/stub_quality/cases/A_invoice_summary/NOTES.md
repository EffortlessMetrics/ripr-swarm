1. Changed behavior: Invoice::summary free-shipping boundary `subtotal >= 5000` -> `> 5000`; Summary.shipping differs at exactly 5000.

2. Ideal discriminating test:
```rust
let mut i = Invoice::new(0); i.add_line(Money(5_000)); assert_eq!(i.summary().shipping, Money(0));
```

3. Shape: &self method on struct with new(); returns struct (Summary); Money newtype with Add trait impl
