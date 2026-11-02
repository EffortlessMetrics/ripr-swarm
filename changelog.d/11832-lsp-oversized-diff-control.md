<!-- section: Internal / CI -->
- `lsp_lifecycle` now has an `oversized` control that a one-file non-Rust
  worktree diff over a lowered `RIPR_MAX_DIFF_CHANGED_LINES` yields the typed
  limited LSP result, and the same diff under a higher budget is not limited
  by that ceiling (#11832).
