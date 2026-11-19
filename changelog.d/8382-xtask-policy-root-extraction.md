<!-- section: Internal / CI -->
- Part A of the `cargo xtask` policy extraction: the lint-policy gate
  (`check-lint-policy` with its clippy-debt collectors) and the CI-lane
  ledger (the CiLedger document model, parser and validators) moved
  verbatim from `xtask/src/main.rs` into `xtask/src/policy_root.rs`, and
  the xtask `automation` feature is split into `policy` and `product`
  halves whose union is unchanged, preparing a slim policy-only xtask
  build (#8382). `check-workspace-shape`, the remaining policy `*_impl`
  checks and `tracked_files` stay in `main.rs` for Part B.
