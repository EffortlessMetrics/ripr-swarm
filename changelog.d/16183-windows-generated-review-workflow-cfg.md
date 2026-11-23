<!-- section: Internal / CI -->
- The `generated_review_workflow` test target now compiles on Windows by gating its unix-only replay helpers and test with `#[cfg(unix)]`; unix test behavior is unchanged (#16183).
