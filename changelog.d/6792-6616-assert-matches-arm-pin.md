<!-- section: Fixed -->
- A changed `match` arm pinned by a test-local `assert_matches!` now reads `exposed` when the test calls the owner with an input that selects that arm and an exact pattern, and the macro's first rule provably panics on a mismatch; broad patterns, other-arm inputs, skipped invocations and non-panicking macros still read as a gap (#6616).
