<!-- section: Fixed -->
- The diff `ripr check` reads from git no longer depends on a user's
  `diff.algorithm`, `diff.indentHeuristic`, `diff.renameLimit` or
  `core.bigFileThreshold`, and its `index` lines carry full blob ids, so two
  machines at the same range see the same changed lines and the same
  `input_identity`. For a git-derived diff the identity changes once: an
  analysis-outcome artifact written by an earlier ripr must be regenerated.
  A diff passed with `--diff` is read as given, so its identity is unchanged
  (#15954).
