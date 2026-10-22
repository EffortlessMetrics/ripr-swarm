<!-- section: Fixed -->
- `cargo test -p ripr --lib` compiles on Windows again: the default-output enumeration test added in #9339 used an import that was gated to unix, so every Windows lib-test run (and the advisory subset's gap-artifacts, Cargo-home and process-ownership steps) failed to build.
- `cargo clippy -p ripr --all-targets -- -D warnings` passes on Windows again: the `pr_comments_github` test module imported `store_raw_copy` unconditionally although only a unix-only test calls it (#9339).
