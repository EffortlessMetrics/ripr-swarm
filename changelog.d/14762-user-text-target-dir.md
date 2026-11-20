<!-- section: Internal / CI -->
- `cargo xtask check-user-text` resolves the built `ripr` binary from cargo's
  own artifact report instead of the running image's directory, so the check
  runs from the relocated driver copy on Windows instead of failing with
  "built ripr binary not found" (#14762).
