# path_utils

1. Changed behavior: `extension("dir/file.")` now returns Some("") instead of None (trailing-dot boundary `>=` became `>`)

2. Ideal discriminating test:
```rust
assert_eq!(extension("dir/file."), None);
```

3. Shape: free fns with lifetime params: extension<'a>(&'a str) -> Option<&'a str>; also tuple-returning split_host_port
