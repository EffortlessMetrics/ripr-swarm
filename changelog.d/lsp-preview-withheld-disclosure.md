<!-- section: Fixed -->
- LSP: Python and TypeScript preview findings that the default `actionable`
  profile withholds are now disclosed instead of reading as a clean file:
  `ripr/analysisStatus` `withheld_findings` counts them (new `preview_count`),
  the pull diagnostic channel names them with the `diagnosticProfile: full`
  route, and the refresh log carries `withheld_preview=N`. Eligibility is
  unchanged; `full` still publishes them as information diagnostics (#11296).
