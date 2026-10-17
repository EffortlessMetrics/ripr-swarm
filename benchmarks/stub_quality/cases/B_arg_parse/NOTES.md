# arg_parse

1. Changed behavior: default `jobs` when -j is absent changed from 1 to 2

2. Ideal discriminating test:
```rust
assert_eq!(parse_args(&["prog".to_string()]).unwrap().jobs, 1);
```

3. Shape: struct ctor taking impl Into<String>; &mut self setter; parse_args(&[String]) -> Result<Opts, ArgError>
