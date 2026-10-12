<!-- section: Fixed -->
- A `#[test]` function under a `cfg` that is false in a test build
  (`#[cfg(any())]`, `#[cfg(not(test))]`) is no longer discovered as a test. It
  can never run, but `ripr check` named it as a test that reaches the changed
  function. Gates ripr cannot evaluate (features, targets) still keep the test. ([#6293](https://github.com/EffortlessMetrics/ripr-swarm/issues/6293), [#6717](https://github.com/EffortlessMetrics/ripr-swarm/pull/6717))
