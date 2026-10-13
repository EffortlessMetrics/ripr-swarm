<!-- section: Fixed -->
- With an absolute `--root`, as the editor sidecar uses, a struct or field
  change in a member crate's `src/lib.rs` no longer lists the workspace root
  crate's `src/lib.rs` inline tests as `same_test_file`. Related tests
  resolve the changed file to its root-relative indexed path, preferring
  the owner function's file, instead of accepting any path that shares its
  tail (#6791).
