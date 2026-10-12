<!-- section: Fixed -->
- Rust oracle grading admits an exact-equality `.any()` membership check
  such as `assert!(lines.iter().any(|l| l == "audited 42"))` as an exact
  value oracle instead of a weak relational check, so the reported grade
  matches what the assertion pins (#6991).
