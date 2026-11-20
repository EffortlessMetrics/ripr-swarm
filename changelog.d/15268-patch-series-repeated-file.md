<!-- section: Fixed -->
- `ripr check --diff` no longer anchors findings at stale lines when a
  `git format-patch` series or `git log -p` output edits one source file in
  more than one commit. That file is now refused with a located limitation and
  the recovery "analyze one diff of the whole change" (`git diff <base>...HEAD`
  or `ripr check --base <base>`); files changed once in the series are still
  analyzed. The report is marked incomplete (`partial_with_limitations`) and
  exits like other partial results (#15268).
