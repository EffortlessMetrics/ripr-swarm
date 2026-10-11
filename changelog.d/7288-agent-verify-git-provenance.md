<!-- section: Fixed -->
- `ripr agent verify` names unavailable Git/revision provenance when genuine
  snapshots come from a Cargo root without a committed Git HEAD or available
  worktree status. `agent brief` and `agent start`'s generated brief
  warn that snapshot verification requires that provenance while saved-patch
  analysis remains supported; Git-backed `--diff` briefs with a null base stay
  warning-free (#7288).
