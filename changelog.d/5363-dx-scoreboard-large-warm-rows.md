<!-- section: Internal / CI -->
- The dx scoreboard's heavy corpus gains `cargo` (its shared-corpus pin) and
  `ripr-swarm-2026-10` (this repository at its current size, pinned at a
  commit that edits product Rust), so the warm-check row also tracks the large
  workspaces where the inner loop is judged. The experience roadmap's speed row
  records that the warm check varies with workspace and diff (#5363, #7220).
