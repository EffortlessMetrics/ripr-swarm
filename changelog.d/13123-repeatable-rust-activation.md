<!-- section: Internal / CI -->
- Add a sourced Bash activation boundary for exact installed Rust releases,
  inherited compiler and home selection, and matching optional LLVM tools.
  Probe the selected Cargo before exports so an unusable executable is refused.
  Refuse selected PATH directories containing colons before changing the shell.
  Resolve relative invocation paths independently of the caller's CDPATH.
  Keep path resolution independent of caller command overrides, and enforce
  helper cleanup and sysroot mismatch regression controls.
  Retain controlled-shell and installed-Rustup nested-fixture identity checks.
  Scope the native JUnit identity receipt to one output-only policy exception.
  Document online provisioning and restored-task activation separately, with
  focused shell regression coverage and explicit limits on readiness claims
  (#13123).
