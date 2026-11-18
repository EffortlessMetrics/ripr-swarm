<!-- section: Fixed -->
- `ripr check` no longer spends time quadratic in line length relating a changed `let` binding to its predicate uses: a 500 KB one-line function body took 18 seconds and now takes 0.2 seconds. The relation's answers are unchanged (#14096).
