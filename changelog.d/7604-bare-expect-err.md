<!-- section: Fixed -->
- Rust oracles: an uncompared `expect_err(..)`, such as
  `let _e = check(x).expect_err("too long");`, reads `broad_error` / weak
  like `unwrap_err()` instead of a medium `mock_expectation`, so it no
  longer overstates the test. A mock's own `expect_*` call still reads as
  a mock expectation
  ([#7604](https://github.com/EffortlessMetrics/ripr-swarm/issues/7604)).
