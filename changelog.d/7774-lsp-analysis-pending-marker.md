<!-- section: Fixed -->
- The LSP pull diagnostic paths no longer answer a pre-analysis workspace
  with an empty report that is indistinguishable from "analysis ran and
  found nothing". Before the first analysis snapshot exists,
  `textDocument/diagnostic` serves one Information-severity
  `ripr-diagnostic-analysis-pending` marker naming the never-analyzed
  state, under a stable `ripr:analysis-pending:<recovery>` result identity
  that answers `unchanged` for clients already holding the marker;
  `workspace/diagnostic` serves the same marker per tracked document URI.
  The marker promises only the recovery route the current blocking state
  actually honors — save a file or run `ripr.refresh` when configuration
  is valid and the workspace root admits analysis, and the root
  authority's own route (select a root and restart) or configuration
  repair when refresh would return before analysis — so a blocked state
  never advertises a route that cannot start analysis. The marker
  retracts automatically when the first snapshot commits and normal
  serving resumes, and the pending identity can never hide that
  transition behind an `unchanged` report (#7774).
