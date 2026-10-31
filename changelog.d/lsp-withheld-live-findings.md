<!-- section: Fixed -->
- LSP: a changed function that no test calls no longer leaves the editor on an
  empty list with `withheld_findings.count = 0`. Candidate-actionable
  `no_static_path`, `weakly_exposed` and `reachable_unrevealed` findings that
  the default `actionable` profile does not publish are now counted in
  `ripr/analysisStatus` `withheld_findings` (new `gap_count`, so `count`
  reconciles with `analysis_outcome.counts.finding_count`), named by the pull
  disclosure and the refresh log (`withheld_gap=N`), and disclosed by the
  existing passive Hint `ripr-diagnostic-preview-withheld` ("ripr: N
  changed-behavior finding(s) with no static test path found or weak static
  evidence not shown under the actionable profile; set diagnosticProfile: full
  or run `ripr check`"). The Hint carries no finding data and offers only
  refresh. Which findings publish, severities and eligibility are unchanged;
  `full` still publishes them with no marker (#11798).
