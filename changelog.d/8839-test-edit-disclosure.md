<!-- section: Fixed -->
- PR review guidance recognizes deleted assertions and inline Rust test edits,
  including removals after earlier line shifts, and preserves unchanged-test
  disclosure for production-only edits in test-shaped filenames without changing
  recommendation classification or deleted-line placement rules. Whole-file
  deletions with unavailable test-role attribution now warn and withhold the
  affirmative unchanged-tests claim (#8839).
