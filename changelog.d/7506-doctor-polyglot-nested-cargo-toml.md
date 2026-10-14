<!-- section: Fixed -->
- Doctor: `ripr doctor` at a polyglot git root that has a TypeScript marker
  (`package.json`) and a nested Cargo workspace (`rust/Cargo.toml`) no longer
  skips `cargo_toml`. The cargo-only locator still names that nested
  manifest; a TypeScript-only root with no nested or ancestor Cargo.toml
  still skips ([#7506](https://github.com/EffortlessMetrics/ripr-swarm/issues/7506)).
