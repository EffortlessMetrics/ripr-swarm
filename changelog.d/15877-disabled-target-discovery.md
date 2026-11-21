<!-- section: Fixed -->
- `ripr check` no longer reports findings in a `src/main.rs`, `src/bin/*.rs` or `src/lib.rs` that `autobins = false` or `autolib = false` leaves out of the build, or in a module only such a file mounts; declared targets, including a path-less `[[bin]]` or `[lib]`, still seed (#15877)
