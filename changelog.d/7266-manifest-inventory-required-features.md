<!-- section: Fixed -->
- `ManifestInventory` now treats a `[[test]]` with `required-features`
  unmet under cargo defaults as not `HarnessEnabled`. `cargo test`
  skips that target even when metadata still reports `kind = ["test"]`.
  The same target stays live when those features are default-enabled.
  Ordinary autodiscovered `tests/<name>.rs` stay credited when metadata
  is unavailable
  ([#7266](https://github.com/EffortlessMetrics/ripr-swarm/issues/7266)).
