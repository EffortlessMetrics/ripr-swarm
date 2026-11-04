<!-- section: Fixed -->
- A scoped `ripr check --path`/`--exclude` run now prints those flags on its
  explain, context, and `check --json` drill-in commands, and `ripr explain`
  and `ripr context` accept the same flags, so a paste re-analyzes the scoped
  files instead of the full oversized diff (#9340).
