<!-- section: Fixed -->
- Rust: a file's own module glob (`mod inner; use inner::*;`) is no longer
  read as a foreign glob because the file also calls std's `thread_local!` or
  `compile_error!`, so its tests keep their `assert_eq!` credit instead of
  being refused with a misleading "glob-imports from outside the workspace"
  reason (#14116).
