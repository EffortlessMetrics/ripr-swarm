<!-- section: Fixed -->
- A Rust test that pins a changed bool field with a bare `assert!(opts(10).big)` or `assert!(!opts(9).big)` now counts as a strong check, just as `assert_eq!(opts(10).big, true)` already did. Both the field and the predicate finding on the changed line can read `exposed`. A condition that compares, combines or calls a method stays weak (#14701).
