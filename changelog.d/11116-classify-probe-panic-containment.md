<!-- section: Fixed -->
- A panic while classifying one Rust probe no longer aborts the whole run
  (#11116). The failed probe becomes an explicit `static_unknown` stand-in
  (every stage unknown, no related test, no gap, no repair signal, with a
  `classification_incomplete` evidence line), the other files' findings are
  kept unchanged, and the analysis outcome carries a `producer_failure`
  limitation naming the file, so the run reads incomplete and the failed probe
  can never be `exposed` or actionable.
