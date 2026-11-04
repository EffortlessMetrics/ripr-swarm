<!-- section: Fixed -->
- Assertion-refusal evidence text now renders file paths with the stable slash spelling on every host, so `cargo xtask goldens check` passes on native Windows instead of drifting on `tests\api.rs`-style separators.
