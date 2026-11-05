<!-- section: Internal / CI -->
- `cargo xtask dx-scoreboard --gate` now fails when every selected metric
  has no current value and none is comparable with the baseline, naming the
  zero-evidence boards in `gate.zero_evidence_boards` instead of passing on
  unmeasured evidence alone (#13101).
