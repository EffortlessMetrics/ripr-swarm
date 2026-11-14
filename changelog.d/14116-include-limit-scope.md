<!-- section: Fixed -->
- One `include!` ripr could not place no longer refuses every `assert!` and
  `assert_eq!` in the workspace as soon as any file globs a foreign crate
  (`use rayon::prelude::*;`). An `include!` of a literal repository path is
  now read: a fragment that mounts no file and can bind no assertion macro
  leaves the other files placed, and one in an integration test, bench or
  example only turns placement off for that crate. Dynamic, missing or
  mounting includes, and a possible binding inside a library, keep the
  workspace-wide refusal, and a test that reaches a fragment's own function
  is still not credited for the owner's (RIPR-SPEC-0197, #14116).
