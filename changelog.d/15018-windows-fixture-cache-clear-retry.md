<!-- section: Internal / CI -->
- `cargo xtask goldens check` and `fixtures` retry a per-fixture cache clear that
  Windows reports as momentarily in use (access denied, sharing violation,
  directory not empty) with a bounded backoff of about 1.6s. A holder that
  outlives it, and every non-Windows or other error, is still reported as a
  fixture error, now naming the attempt count and raw OS error (#15018).
