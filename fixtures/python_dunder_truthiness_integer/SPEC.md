# Fixture: python_dunder_truthiness_integer

Spec: RIPR-SPEC-0238

## Given

An authored plain integer constructor and a one-comparison `Money.__bool__`
change from `self.cents != 0` to `self.cents > 0` (#9795).
The test imports `Money` from `shapes.money` and asserts `Money(-1)`.

## When

`cargo xtask fixtures python_dunder_truthiness_integer`

## Then

The single finding is `exposed`: `assert Money(-1)` passes before the edit and fails after it.

## Must Not

Credit unresolved primitive values, unchanged truth values, or a class imported
from another module. Equality guards and string conversion stay outside this slice.
