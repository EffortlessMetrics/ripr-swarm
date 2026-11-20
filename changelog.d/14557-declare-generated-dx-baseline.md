<!-- section: Internal / CI -->
- `.gitattributes` now declares the canonical `metrics/dx-scoreboard/baseline.json`
  report as `linguist-generated`, so candidate admission stops counting a promoted
  scoreboard baseline as hand-written review scope. Only that exact file is
  exempt; neighboring baselines and `benchmarks/dx_scoreboard/scoreboards.toml`
  stay counted, and the 40,000-line generated cap still applies (#14557).
