<!-- section: Internal / CI -->
- `cargo xtask ripr-pr` retires the expensive canonical snapshot stage,
  historically costing about 246 seconds, while writing a cheap explicit
  unavailable comparison at `target/ripr/pr/canonical-delta.json` to preserve
  later gate behavior. Stale comparisons are replaced; the compatibility
  artifact is not a required evidence member or a completed analysis (#13577).
