<!-- section: Fixed -->
- Rust oracles: an uncompared `expect_err(..)`, such as
  `let _e = check(x).expect_err("too long");`, reads `broad_error` / weak
  like `unwrap_err()` instead of a medium `mock_expectation`, so it no
  longer overstates the test. A mock's own `expect_*` call still reads as
  a mock expectation, including a zero-argument `expect_err()` on a call
  receiver on both adapters. `expect_` text inside a string or comment no
  longer counts as a mock expectation name
  ([#7604](https://github.com/EffortlessMetrics/ripr-swarm/issues/7604)).
