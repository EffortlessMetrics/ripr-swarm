<!-- section: Internal / CI -->
- `cargo xtask check-lint-policy` checks string and table-form Cargo lint
  levels against the active, planned, and debt ledgers, rejects invalid
  manifest or level syntax, and records the existing `unexpected_cfgs`
  setting in the active Rust ledger (#13156).
