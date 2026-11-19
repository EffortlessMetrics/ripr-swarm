<!-- section: Fixed -->
- `ripr check`: when a `match` scrutinee edit leaves a comparison inside it
  untouched, ripr now drops that comparison's finding only when the old line
  held it as a whole operand at least as often as the new line. A comparison
  that was part of a longer expression before the edit (`a > b as u8`,
  `m | a > b`, `x.a > b`), or one the edit repeated, is still reported.
  Closure parameters, comments and ranges beside the comparison do not count
  as part of the operand (#14722).
