<!-- section: Fixed -->
- Rust: a result-side oracle no longer earns `exposed` when test code calls `process::exit`, `abort` or `.exec()` in an owner-call argument, a check helper or a table loop body, since that call can end the test run before the assertion (#8015).
