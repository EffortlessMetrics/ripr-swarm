# kv_config

1. Changed behavior: `KvPair::from_str("=v")` now returns `MissingSeparator` instead of `MissingKey` for an empty key

2. Ideal discriminating test:
```rust
assert_eq!("=v".parse::<KvPair>(), Err(ParseError::MissingKey));
```

3. Shape: FromStr impl (trait impl) returning Result<KvPair, ParseError> (custom error enum), takes &str
