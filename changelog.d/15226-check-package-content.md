<!-- section: Internal / CI -->
- `cargo xtask check-package-content` gates the `cargo package --list` output
  for `ripr` and `ripr-contract`, failing closed when a listing cannot be
  obtained and flagging test-only paths (`src/testing/**`, `tests.rs`,
  `tests/**`, non-allowlisted `*_tests.rs`, `corpus_tests`, Kani proof files)
  so test scaffolding cannot ship unnoticed in the published crates (#15226).
