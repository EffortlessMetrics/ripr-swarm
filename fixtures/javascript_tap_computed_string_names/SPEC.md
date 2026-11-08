# Fixture: javascript_tap_computed_string_names

Spec: RIPR-SPEC-0085
Spec: RIPR-SPEC-0108

## Given

Tap named registrations use immutable computed string names.

## When

The stringify predicate is inverted; run `cargo xtask fixtures javascript_tap_computed_string_names`.

## Then

The two active tests retain strong exact-value evidence and exposed classification.

## Must Not

- Credit an unexecuted callback.
- Treat the static result as a native runtime verdict.
