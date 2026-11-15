<!-- section: Internal / CI -->
- `cargo xtask` policy implementations (`check-lint-policy`, the CI-lane
  ledger, `check-workspace-shape`, and the static-language/allow-attribute/
  local-context/process/network policy checks) moved verbatim from
  `xtask/src/main.rs` into `xtask/src/policy_root.rs`, and the xtask
  `automation` feature is split into `policy` and `product` halves whose
  union is unchanged, preparing a slim policy-only xtask build (#8382).
