<!-- section: Fixed -->
- JavaScript and TypeScript: replacing a statement with an empty statement
  (`queue.clear();` -> `;`) is now checked as a removal. `ripr check` used to
  report `0 changed behaviors checked`; it now records the removed statement
  as base-side evidence with its static class, the same as a deleted line.
  Removed code is still never an edit target (#15335).
