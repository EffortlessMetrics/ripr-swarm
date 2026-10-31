<!-- section: Internal / CI -->
- `cargo xtask schema-producer-sweep --live-replay` replays the exact-cut schema packet in one command: it runs `doctor`, `pr-evidence`, `review-comments`, `ripr agent repair` and `verify-execute` from a binary that must report the replayed commit, and validates each against its published schema (#3920).
