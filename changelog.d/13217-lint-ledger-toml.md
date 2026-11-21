<!-- section: Internal / CI -->
- `cargo xtask check-lint-policy` decodes lint-ledger rows as TOML, rejects
  invalid known fields and duplicate identities, and retains commented
  planned rows in the MSRV and debt checks. Rust and Clippy lint names
  keep separate ledger coverage; documentary extension fields remain
  supported (#13217).
