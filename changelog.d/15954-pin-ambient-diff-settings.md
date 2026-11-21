<!-- section: Fixed -->
- The diff `ripr check` analyzes no longer depends on a user's
  `diff.algorithm`, `diff.indentHeuristic`, `diff.renameLimit` or
  `core.bigFileThreshold`, and its `index` lines carry full blob ids, so two
  machines at the same range see the same changed lines and the same
  `input_identity`. The identity changes once for every repository: an
  analysis-outcome artifact written by an earlier ripr must be regenerated
  (#15954).
