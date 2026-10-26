<!-- section: Internal / CI -->
- `Expect` and `Route` in `xtask/src/termination_qualification/controls.rs` are now imported under `#[cfg(unix)]`, since only the unix-only instrument controls consume them; strict `cargo clippy -- -D warnings` on Windows no longer trips over the unused import (#10624).
