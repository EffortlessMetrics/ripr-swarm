<!-- section: Internal / CI -->
- `cargo xtask check-fast`, `ci-fast`, `precommit` and `shape` format the
  workspace as bounded `rustfmt` batches instead of one `cargo fmt` command
  line, so the formatting step runs on Windows again now that a package's
  target roots outgrow the 32,767-character limit (#12004). The step runs
  the same formatter `cargo fmt` would, refuses a batch Windows could not
  start before any batch runs, and asks again for a metadata reply it cannot
  plan from.
