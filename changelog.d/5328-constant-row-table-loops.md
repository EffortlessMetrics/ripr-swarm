<!-- section: Fixed -->
- An `assert_eq!` inside a `for` loop over a non-empty table of constant
  rows (`for (input, want) in [(1, 2), (3, 6)] { .. }`, inline or bound once
  by a plain `let`) now counts as running, so a table-driven test's exact
  pin is credited instead of reading "assertion not credited". Ranges,
  computed or possibly empty iterables, rows that call a function or name a
  `const`, and a `break`/`continue` before the assertion stay refused
  (RIPR-SPEC-0197, #5328).
