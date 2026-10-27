<!-- section: Added -->
- MCP `ripr_refresh` accepts an optional `source` argument (`committed`, the
  default, or `worktree`). `worktree` analyzes saved staged and unstaged
  tracked edits through the shared worktree check without committing them.
  The snapshot is tagged and its identity binds the source, requested and
  captured source stay distinct across a failed refresh, and repair cards
  and repair readiness are withheld for worktree snapshots because no
  consumed-source commitment spans their findings (unsaved buffers and
  untracked files stay outside) (#7357).
