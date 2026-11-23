<!-- section: Fixed -->
- `ripr reports ci-packet` now captures `target/ripr/reports/pr.diff` with
  the same pinned git command as analysis, so a user's rename limit, diff
  algorithm, indent heuristic, big-file threshold or abbreviation setting no
  longer changes the recorded patch, and its `index` lines carry full blob
  ids (#9365).
