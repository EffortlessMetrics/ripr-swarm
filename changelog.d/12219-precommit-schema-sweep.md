<!-- section: Internal / CI -->
- `cargo xtask precommit` now runs `schema-producer-sweep --worktree` when a schema or JSON document changes, so a golden carrying a member its schema forbids fails locally instead of at the hosted Rust gates (#12219).
