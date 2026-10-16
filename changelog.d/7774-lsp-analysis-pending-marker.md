<!-- section: Fixed -->
- The LSP pull diagnostic paths no longer answer a pre-analysis workspace
  with an empty report that is indistinguishable from "analysis ran and
  found nothing". Before the first analysis snapshot exists,
  `textDocument/diagnostic` serves one Information-severity
  `ripr-diagnostic-analysis-pending` marker naming the never-analyzed
  state and the save-or-`ripr.refresh` recovery route, under a stable
  `ripr:analysis-pending` result identity that answers `unchanged` for
  clients already holding the marker; `workspace/diagnostic` serves the
  same marker per tracked document URI. The marker retracts automatically
  when the first snapshot commits and normal serving resumes, and the
  pending identity can never hide that transition behind an `unchanged`
  report (#7774).
