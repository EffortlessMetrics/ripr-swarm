<!-- section: Fixed -->
- A changed root `build.rs` no longer relates tests by name token. A test named
  for a token of the changed expression (even one inside a string literal) used
  to make the finding `reachable_unrevealed` with an "add an assertion" next
  step, although no package test can run a build script (#9173).
