<!-- section: Fixed -->
- Doctor: `ripr doctor` at the git root of a repo whose Cargo workspace lives
  in a subdirectory (`rust/Cargo.toml`) passes the `cargo_toml` check and names
  that nested manifest, matching `ripr check` from the same cwd. An empty root
  or Rust sources with no Cargo.toml still fail
  ([#7466](https://github.com/EffortlessMetrics/ripr-swarm/issues/7466)).
