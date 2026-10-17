<!-- section: Fixed -->
- Rust oracles: an uncompared `.expect_err (` with space before `(` and
  UFCS `Result::expect_err(<call>, <msg>)` now read `broad_error` / weak
  like `.expect_err("…")`, instead of a medium `mock_expectation`. Empty
  `expect_err()` / `expect_err( )` still read as a mock expectation. A
  line that starts with `expect_err(` no longer overflows in debug
  ([#7979](https://github.com/EffortlessMetrics/ripr-swarm/issues/7979)).
