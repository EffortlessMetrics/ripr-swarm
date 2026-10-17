# semver_lite

1. Changed behavior: Display of a prerelease version uses '+' instead of '-' separator ("1.2.3+beta" instead of "1.2.3-beta")

2. Ideal discriminating test:
```rust
assert_eq!(Version::new(1, 2, 3).with_pre("beta").to_string(), "1.2.3-beta");
```

3. Shape: Display impl (trait impl) on struct with Option<String> field; From<(u32,u32,u32)> tuple impl; &mut self bump
