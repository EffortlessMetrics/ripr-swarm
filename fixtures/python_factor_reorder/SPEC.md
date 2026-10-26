# Fixture: python_factor_reorder

Spec: RIPR-SPEC-0238

## Given

Generic product functions and exact table oracles using integer and finite
float inputs, including products inside tuples and beside a quoted hash.

## When

Its factors swap. Python names and int/float annotations can still receive
noncommutative user objects; the numeric table does not establish global equivalence.

## Then

The finding stays weakly_exposed through the ordinary unvaried-table reading.

## Must Not

Credit a named factor reorder by assuming multiplication commutes. The
RIPR-SPEC-0108 honesty corpus independently pins the rendered classification.
`runtime_control.py` supplies native Python numeric and overloaded-operator controls.
