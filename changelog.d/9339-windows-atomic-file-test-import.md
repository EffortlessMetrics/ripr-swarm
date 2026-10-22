<!-- section: Fixed -->
- `cargo test -p ripr --lib` compiles on Windows again: the default-output enumeration test added in #9339 used an import that was gated to unix, so every Windows lib-test run (and the advisory subset's gap-artifacts, Cargo-home and process-ownership steps) failed to build.
