<!-- section: Added -->
- MCP `ripr_refresh {"source":"worktree"}` now offers repair readiness,
  `ripr_get_repair_card` and `ripr_prepare_repair` for saved tracked edits when
  the findings producer and the seam inventory are shown to have consumed the
  same Rust source bytes. Each producer records a SHA-256 of the raw buffer it
  loads before parsing or cache access; the shared source authority compares
  them, and a missing, conflicting, out-of-root or moved observation, or a
  `HEAD` that moved between the producers, withholds cards and readiness with
  the cause named instead of pairing two saved states. The producer commitment
  is part of the worktree snapshot identity, so an equal-result edit to a
  consumed helper or test invalidates pinned snapshots. `scope`
  `consumed_source_commitment` is `established` (with `repair_cards:
  "available"`) or `not_established` (`withheld`); unsaved buffers, untracked
  files and non-Rust inputs stay outside (#7357).
