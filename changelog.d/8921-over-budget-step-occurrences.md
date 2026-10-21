<!-- section: Internal / CI -->
- DX scoreboard: `first_run.over_budget_steps` ingestion now counts step
  occurrences instead of breached metric rows (before: one step breaching
  both its time and stdout budgets counted 2; after: it counts 1, with both
  evidence entries retained), aligning ingestion with the public-proof
  renderer and the metric's declared `steps` unit (#8921). A missing
  `stdout_lines` row no longer merges the next occurrence into the
  first, and baselines recorded at the old breached-row grain are not
  compared for this metric until re-recorded.
