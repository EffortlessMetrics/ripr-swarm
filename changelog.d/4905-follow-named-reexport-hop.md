<!-- section: Fixed -->
- Rust checks: a sibling-crate test that imports the owner's type through its own library's `pub use` re-export is now related to the owner, including one crate-local hop first (tracing-subscriber's `filter::LevelFilter` re-exported from `tracing_core` via `filter/level.rs`). Longer or ambiguous chains still stay unrelated (#4905).
