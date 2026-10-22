<!-- section: Changed -->
- A `map(..).collect::<Result<Vec<_>, _>>()` owner tail pinned by an exact `assert_eq!` on a non-empty literal input now reads `exposed` through RIPR-SPEC-0265, and plain string literals count as run elements. `Result` is held to the same rebinding check as `Vec`; an empty input keeps the earlier reading (#9490).
