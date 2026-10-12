<!-- section: Fixed -->
- Rust analysis no longer credits tests Cargo never builds: a test file no
  `mod` declares, or an integration test that `autotests = false` leaves
  unregistered, earns no related-test credit
  ([#6965](https://github.com/EffortlessMetrics/ripr-swarm/issues/6965)).
  Module-tree orphan proofs now also run when ripr is invoked from the
  workspace directory itself.
