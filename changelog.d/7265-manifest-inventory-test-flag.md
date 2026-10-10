<!-- section: Fixed -->
- `ManifestInventory` now carries cargo metadata's per-target `test`
  flag, so a declared `[[test]]` with `test = false` is not
  `HarnessEnabled`. `cargo test` skips that target even when metadata
  still reports `kind = ["test"]`, including `path = "tests/../tests/gate.rs"`
  spellings cargo normalizes. Ordinary autodiscovered `tests/<name>.rs`
  stay credited when metadata is unavailable
  ([#7265](https://github.com/EffortlessMetrics/ripr-swarm/issues/7265)).
