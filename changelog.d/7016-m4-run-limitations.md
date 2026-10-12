<!-- section: Fixed -->
- `cargo xtask bench-agent-surfaces` M4 no longer presents a
  byte-budget-capped findings subset as the complete population. The M4
  block now reads each pooled envelope's top-level `run_limitations` and
  reports the emitted-vs-reported denominator: `findings_total` stays the
  emitted count, `findings_reported` sums `summary.findings` (null when any
  pooled envelope omits it), `envelopes_findings_bound` counts envelopes
  disclosing `limited_findings_bound`, and `max_run_limitations` forwards
  the run-level limitation count alongside the unchanged
  `max_typed_limitations` analysis-outcome path. The Markdown M4 line
  carries the same disclosure (#7016).
