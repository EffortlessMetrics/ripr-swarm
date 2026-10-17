<!-- section: Fixed -->
- Diff-scoped SARIF (`ripr check --format sarif`) records
  `runs[].properties.omitted_not_current` when the candidate-actionable
  filter drops base-side or unresolved findings, so an all-omitted
  complete run is not indistinguishable from a clean analysis (#5091).
