# http_status

1. Changed behavior: `Status::GatewayTimeout.is_retryable()` now false (variant removed from matches! pattern)

2. Ideal discriminating test:
```rust
assert!(Status::GatewayTimeout.is_retryable());
```

3. Shape: enum method (&self) with matches! over enum variants; from_code match -> Option<Status>; Display impl
