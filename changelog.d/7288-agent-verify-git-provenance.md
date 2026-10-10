<!-- section: Fixed -->
- `ripr agent verify` names unavailable Git/revision provenance when genuine
  snapshots come from a Cargo root without a committed Git HEAD. `agent brief`
  warns that snapshot verification requires that provenance while saved-patch
  analysis remains supported; Git-backed `--diff` briefs with a null base stay
  warning-free (#7288).
