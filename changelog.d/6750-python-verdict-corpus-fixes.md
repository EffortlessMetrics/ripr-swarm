<!-- section: Fixed -->
- Python verdicts follow RIPR-SPEC-0238. A self-comparison, a mixed `==`
  chain or a `match=` pattern that accepts any message no longer credits
  `exposed`. Credit comes from one assertion that observes the owner, in
  any position, and never from an assertion of another behavior family. A
  method owner is identified only through a receiver bound to its class. An
  expected value computed through the changed owner does not credit, and a
  changed raise is credited only by an assertion that pins the exception
  (#6600).
- Python credits an `and` chain of exact comparisons, a boundary written as
  `CONSTANT - 1`, and an assertion that reads, after the owner runs, the
  output, file, mock call or field a changed line writes. These earlier
  read as actionable gaps (#6599).
- Hypothesis `@given` parameters are no longer reported as unresolved
  pytest fixtures, and a readable `@example` row pins a boundary like a
  `parametrize` case. A strong `@given` test whose body reads a generated
  input and has no readable `@example` row now reads `static_unknown`
  instead of `exposed`, since its inputs are not visible statically (#6601).
