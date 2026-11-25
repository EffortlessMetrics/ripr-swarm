<!-- section: Fixed -->
- `assert_eq!(matches!(value, PATTERN), true)` in Rust tests is now only as strong as its pattern, as `assert!(matches!(value, PATTERN))` already was. A wildcard, side-only (`Some(_)`) or several-value pattern no longer makes a change read `exposed` when it is written as an equality with `true`; a pattern that pins one value stays exact (#16290).
