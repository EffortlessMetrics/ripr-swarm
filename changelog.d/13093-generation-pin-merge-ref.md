<!-- section: Internal / CI -->
- `cargo xtask check-generation-pins` evaluates the original pull-request head
  and fork on synthetic merge refs, so a generation already claimed by a merged
  sibling remains a collision instead of being treated as unchanged (#11736).
