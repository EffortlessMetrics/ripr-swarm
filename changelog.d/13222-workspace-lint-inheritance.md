<!-- section: Internal / CI -->
- `cargo xtask check-lint-policy` requires every Cargo workspace member to
  inherit the shared lint policy, rejecting missing or replaced opt-ins while
  preserving read-only policy scans (#13222).
