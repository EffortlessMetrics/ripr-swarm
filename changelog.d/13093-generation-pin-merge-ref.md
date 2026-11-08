<!-- section: Internal / CI -->
- `cargo xtask check-generation-pins` evaluates the original pull-request head
  and fork on synthetic merge refs, so a generation already claimed by a merged
  sibling remains a collision even when the fetched base advances. Invalid
  hosted merge identity or ancestry fails closed instead of silently using the
  merge's working tree (#11736).
