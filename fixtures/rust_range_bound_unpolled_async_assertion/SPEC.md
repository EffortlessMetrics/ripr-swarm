# Fixture: rust_range_bound_unpolled_async_assertion

Spec: RIPR-SPEC-0108

## Given

A test that asserts `sum_to(0) == 0`, plus an `assert_eq!(sum_to(4), 10)` inside an async block the test never polls.

## When

`sum_to` changes its range from `(0..n).sum()` to `(0..=n).sum()`. Only `sum_to(0)` runs, and both versions return `0` there.

## Then

The finding is not `exposed`: no asserted input that the test runs distinguishes the old and new range bounds.

## Must Not

Credit the separating input `4` from an assertion inside an async block that never runs (#14538). The honesty corpus independently rejects the false promotion after re-blessing.
