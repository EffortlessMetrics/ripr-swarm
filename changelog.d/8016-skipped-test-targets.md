<!-- section: Fixed -->
- Rust analysis no longer credits tests in a `[[test]]` target that
  `cargo test` never runs: `test = false`, `harness = false` or
  `required-features` unmet under cargo defaults
  ([#8016](https://github.com/EffortlessMetrics/ripr-swarm/issues/8016)).
