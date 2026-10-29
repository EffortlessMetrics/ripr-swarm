<!-- section: Fixed -->
- `ripr agent start --seam-id` now refuses a seam that `ripr repair` and
  `agent card` report as not repair-eligible (route not ready, no editable
  test target) with the typed `seam_not_actionable` refusal (exit 3) instead
  of writing `workflow.json`, `commands.md` and `agent-brief.json` and exiting
  0. It reuses the shared repair-admission gate, so start, repair and card
  agree (#10712).
