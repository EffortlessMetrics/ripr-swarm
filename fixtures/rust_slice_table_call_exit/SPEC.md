# Fixture: ordinary call inside an assertion skips a slice row

Spec: RIPR-SPEC-0001

## Given

The retained grid subject uses a nonempty slice followed by an empty slice.
After the first owner call, a standard assertion operand calls
`std::process::exit(0)`. That call terminates the test process successfully
before the empty row is passed to the owner. It is not an explicit return,
try expression or opaque macro invocation.

## When

`ripr check` analyzes the `samples.len() == 0` predicate edit.

## Then

The finding retains `infection_unknown`: ordinary calls whose effects are
unresolved withhold this bounded cardinality producer, including calls inside
otherwise trusted assertion operands.

## Must Not

Credit the skipped empty row as observed equality or claim a missing boundary
from a partially executed table. Removing the terminating statement restores
ordinary eager-table evidence; disabling the guard then fails at the empty row.
