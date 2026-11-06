<!-- section: Internal / CI -->
- `cargo clippy -p xtask --all-targets -- -D warnings` passes on Windows
  again: the Unix-only `sample_of` test helper in `dx_scoreboard` tests is
  cfg-gated with its `#[cfg(unix)]` callers instead of surfacing as dead
  code (#12947).
