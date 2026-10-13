<!-- section: Fixed -->
- `cargo xtask check-fast` validates applicable support-tier and spec declarations
  before compilation-bearing Rust gates, catching malformed no-impact declarations
  locally while retaining unrelated-path skips and existing hosted routing (#7528).
