<!-- section: Internal / CI -->
- `cargo xtask ripr-pr` no longer spends about 246 seconds producing
  unavailable canonical snapshots or emitting the constant
  `target/ripr/pr/canonical-delta.json`; stale copies are removed, while
  optional consumers retain compatibility with historical or externally
  supplied artifacts (#13577).
