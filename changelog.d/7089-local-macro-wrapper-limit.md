<!-- section: Fixed -->
- A test-local `macro_rules!` assertion wrapper that is not spelled `assert_*` (such as `check_eq!`) beside a proximity-only oracle on another function now reads as `rust_macro_wrapped_assertion_unresolved` instead of a confident `weakly_exposed` gap (#7089).
