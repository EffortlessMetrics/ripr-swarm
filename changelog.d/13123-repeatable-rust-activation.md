<!-- section: Internal / CI -->
- Add a sourced Bash activation boundary for exact installed Rust releases,
  inherited compiler and home selection, and matching optional LLVM tools.
  Refuse selected PATH directories containing colons before changing the shell.
  Resolve relative invocation paths independently of the caller's CDPATH.
  Document online provisioning and restored-task activation separately, with
  focused shell regression coverage and explicit limits on readiness claims
  (#13123).
