<!-- section: Changed -->
- `ripr continue` on an attempt that is already complete now prints a short
  summary (the receipt, whether a test run was recorded, `ripr repair` for
  the next gap, and the `ripr status --attempt` command for the full page)
  instead of the whole status page. `--json` still prints the attempt's
  status document (#15282).
