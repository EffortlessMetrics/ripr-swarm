<!-- section: Fixed -->
- A Rust test whose check is `debug_assert!`, `debug_assert_eq!` or `debug_assert_ne!` is now read as asserting. These macros run under `cargo test`'s default debug profile, so such a test no longer reads as `reachable_unrevealed` with "has no assertion" when it does observe the changed value (#8051).
