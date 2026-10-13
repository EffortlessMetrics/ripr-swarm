<!-- section: Fixed -->
- Under `--root`, a change in a member crate's `src/lib.rs` no longer lists
  the workspace root crate's `src/lib.rs` inline tests as `same_test_file`.
  Related tests now resolve the changed file to its indexed root-relative
  path instead of accepting any path that shares its tail (#6791).
