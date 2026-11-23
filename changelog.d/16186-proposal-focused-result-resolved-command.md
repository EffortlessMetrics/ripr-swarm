<!-- section: Fixed -->
- Repair: when an attempt follows a new-test proposal for a Rust integration
  file, the start now prints the exact command that produces its focused
  result, through the file's own package manifest and test target
  (`cargo +nightly test --manifest-path M --test T -- -Z unstable-options --format json`),
  so a workspace member that is not a default member runs the new test.
  Inline tests and unresolved targets keep the generic guidance (#16186).
