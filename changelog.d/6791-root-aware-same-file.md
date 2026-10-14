<!-- section: Fixed -->
- With an absolute `--root`, as the editor sidecar uses, a struct or field
  change in a member crate's `src/lib.rs` no longer lists the workspace root
  crate's `src/lib.rs` inline tests as `same_test_file`. Related tests strip
  the analysis root from the changed file's path and compare the
  root-relative result exactly, instead of accepting any path that shares
  its tail (#6791).
