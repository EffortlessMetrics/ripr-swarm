<!-- section: Changed -->
- Repair: a new function nothing in the workspace calls is now a repair start
  on every surface. `ripr check` prints its `ripr repair <id>` line, bare
  `ripr repair` starts it instead of refusing, and `ripr pilot` and the repair
  card offer the same start. A function other code calls but no test reaches
  still gets no start (#15926).
