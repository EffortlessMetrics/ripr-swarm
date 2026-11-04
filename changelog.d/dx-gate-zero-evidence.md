<!-- section: Internal / CI -->
- `cargo xtask dx-scoreboard --gate` now fails when no selected metric is
  comparable with the baseline, naming the zero-evidence boards in
  `gate.zero_evidence_boards` instead of passing on uncompared evidence alone.
