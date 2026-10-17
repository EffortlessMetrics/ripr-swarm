<!-- section: Fixed -->
- `ripr check` on a fresh single-branch repository no longer advises
  `--base <ref>` when no other branch could serve as that base; the
  Start-here action (commit a change, or `--worktree`) is the single
  remediation for the empty-diff state (#7644).
