<!-- section: Fixed -->
- `cargo xtask goldens check` drift artifacts now record which build
  produced the actual side. The console failure card and
  `target/ripr/reports/golden-drift.{md,json}` carry the resolved ripr
  binary path, its `ripr --version` line (version, build commit, dirty
  state), and the checkout revision, once in the report header and in
  each JSON drift entry. A drift entry can now be tied to the build that
  generated it instead of leaving CI-versus-local triage to guesswork.
  Every field fails soft to an explicit `unavailable` note (#7401).
