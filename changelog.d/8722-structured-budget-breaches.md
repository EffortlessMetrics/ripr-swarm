<!-- section: Added -->
- The `first_run.v1` receipt now records a structured `budget_breaches` list
  per step (`metric`, `value`, `budget` for `secs`, `stdout_lines` and
  `workflow_lines`), so a budget bar counts breaches instead of matching the
  wording of the human-readable friction prose. The public-proof renderer
  counts the structured fields when present and falls back to the prose match
  for receipts written before this field existed, so the committed first-run
  receipts keep rendering until the next walk refresh (#8722).
