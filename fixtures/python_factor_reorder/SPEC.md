# Fixture: python_factor_reorder

Spec: RIPR-SPEC-0238

## Given

A generic product function and an exact table oracle using only integer inputs.

## When

Its factors swap. Python names and int/float annotations can still receive
noncommutative user objects; the numeric table does not establish global equivalence.

## Then

The finding stays weakly_exposed through the ordinary unvaried-table reading.

## Must Not

Credit a named factor reorder by assuming multiplication commutes. The
RIPR-SPEC-0108 honesty corpus independently pins the rendered classification.
`runtime_control.py` supplies native Python numeric and overloaded-operator controls.
