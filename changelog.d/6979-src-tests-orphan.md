<!-- section: Fixed -->
- A test file under a `src/tests/` module directory that no `mod` declares is no
  longer credited to the changed function. Cargo never compiles it, but `ripr
  check` treated `src/` as the file's package, found no manifest there and
  kept the credit. A `tests/`, `benches/` or `examples/` directory now only
  starts a Cargo layout when a manifest sits beside it. ([#6979](https://github.com/EffortlessMetrics/ripr-swarm/issues/6979))
