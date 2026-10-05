- Python verdicts follow RIPR-SPEC-0233: a self-comparison, a mixed `==`
  chain or a `match=` pattern that accepts any message no longer credits
  `exposed`; credit comes from one assertion that observes the owner, in any
  position; a method owner is credited only through a bound receiver; and an
  expected value computed through the changed owner does not credit (#6600).
- Python credits an `and` chain of exact comparisons, a boundary written as
  `CONSTANT - 1`, and an assertion on the output, file, mock call or field a
  changed line writes, which earlier read as actionable gaps (#6599).
- Hypothesis `@given` parameters are no longer reported as unresolved pytest
  fixtures, and `@example` rows pin a boundary like `parametrize` cases (#6601).
