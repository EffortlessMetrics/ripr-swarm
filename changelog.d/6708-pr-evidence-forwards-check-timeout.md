<!-- section: Fixed -->
- `cargo xtask ripr-pr` now passes its producer wall-clock budget to
  `ripr check --timeout`, so CI's 3600s PR-evidence ceiling is not
  overridden by the 600s `RIPR_CHECK_TIMEOUT` default (#6708).
