<!-- section: Internal / CI -->
- `metrics/dx-scoreboard/baseline.json` now seeds
  `trust.verdict_corpus_spec_example_coverage` from a real
  `cargo xtask dx-scoreboard --boards trust` producer run (330/330,
  value 1.0), so the gate compares it instead of reporting it absent. (#13318)
