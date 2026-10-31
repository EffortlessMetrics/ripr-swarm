<!-- section: Fixed -->
- LSP: Python and TypeScript preview findings that the default `actionable`
  profile withholds no longer leave the editor on an empty list. Each such
  file now carries one Hint diagnostic, `ripr-diagnostic-preview-withheld`
  ("preview: N preview python finding(s) not shown in the editor; set
  diagnosticProfile: full or run `ripr check`"), anchored on the first
  withheld finding. It has no repair data or actions and sits outside the
  diagnostic budget. `ripr/analysisStatus` `withheld_findings` also counts
  them (new `preview_count`), the pull channel names them, and the refresh
  log carries `withheld_preview=N`. Eligibility is unchanged; `full` still
  publishes the real information diagnostics (#11296).
