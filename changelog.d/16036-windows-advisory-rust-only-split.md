<!-- section: Internal / CI -->
- The Windows Advisory Rust-only (`--no-default-features --features lang-rust`)
  tests now run in parts that each fit their deadline: unit targets and doctests
  in `windows-advisory`, and each `windows-advisory-integration` part's third of
  the integration targets. `cargo xtask windows-advisory-feature-rows` prints a
  row per part and a joined row, and reports a part cut at its deadline as
  incomplete evidence rather than a test failure (#16036).
