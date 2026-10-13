# Fixture: discarded fallible closure skips a slice row

Spec: RIPR-SPEC-0001

## Given

The retained grid subject and predicate edit use a literal slice table with a
nonempty row followed by an empty row. The test discards a fallible closure's
result. A `?` after the first owner call exits before the empty row is passed.

## When

`ripr check` analyzes `samples.len() == 0`.

## Then

The finding retains `infection_unknown`: this bounded table reader cannot
establish every input cardinality in a function containing a try expression.

## Must Not

Credit the unexecuted empty row as observed equality, or fabricate a missing
boundary from a partially executed table. No assertion is needed to expose
this activation-authority defect.
