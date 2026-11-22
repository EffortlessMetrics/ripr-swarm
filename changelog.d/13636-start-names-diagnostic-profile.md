<!-- section: Fixed -->
- Editor: under the default `actionable` diagnostic profile (or with spot
  diagnostics off), "Start repair: copy repair command" is listed disabled on
  a gap finding with the reason and next step: set `ripr.diagnosticProfile`
  to `full` and `ripr.seamDiagnostics` on, then run "ripr: Refresh Full
  Analysis". "ripr: Start Current
  Repair" shows that reason instead of "No current ripr repair gap" (#13636).
