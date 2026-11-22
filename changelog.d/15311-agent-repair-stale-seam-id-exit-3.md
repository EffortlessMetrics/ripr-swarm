<!-- section: Fixed -->
- `ripr agent repair --phase before --seam-id <id>` with a stale `probe:`,
  `repo-probe:`, `gap:` or `file:line` id now exits `3` (the typed
  unknown-seam refusal), as a plain unknown seam id and `agent card`,
  `agent stub`, `agent start` and `agent packet` already did and as
  docs/EXIT_CODES.md states; it used to exit `2` (#15311).
