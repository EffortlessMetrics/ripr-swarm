# Fixture: rust_return_value_inline_tail_if_arm

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15274

## Given

`find_even` ends in a one-line `if` whose condition `x % 2 == 0` is
unchanged, and the edit replaces the then-arm value `Some(x)` with `None`.
The else-arm `Some(0)` is unchanged. One test calls the owner with an even
input and pins the result: `assert_eq!(find_even(4), None)`. It fails on the
old code and passes on the new.

## When

`cargo xtask fixtures rust_return_value_inline_tail_if_arm` runs the public diff analyzer.

## Then

The line carries one `return_value` finding on the changed arm `None`, and it
reads `exposed`, as the same edit does when rustfmt puts each arm on a line
of its own.

## Must Not

- Report a `predicate` finding for the unchanged condition.
- Probe the unchanged else-arm `Some(0)` instead of the changed arm.
