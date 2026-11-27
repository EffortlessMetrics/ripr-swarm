<!-- section: Fixed -->
- A per-path diff driver's algorithm (a `diff=<driver>` attribute with
  `diff.<driver>.algorithm` set in personal Git config) no longer changes
  the lines `ripr check` analyzes or its `input_identity`. The hunk
  algorithm is now pinned with the `--diff-algorithm` flag, which a driver
  cannot override (#15954).
