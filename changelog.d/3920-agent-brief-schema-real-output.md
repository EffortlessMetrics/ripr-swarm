<!-- section: Changed -->
- `ripr agent brief --json` output is now checked against
  `schemas/ripr/agent-brief.schema.json` for every working-set selector
  (`--files`, `--base`, `--diff`, `--seam-id`, `--line-ranges`,
  `--max-seams`). Output is unchanged (#3920).
