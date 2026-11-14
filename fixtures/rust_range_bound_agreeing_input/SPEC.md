# Fixture: rust_range_bound_agreeing_input

Spec: RIPR-SPEC-0108

## Given

A test that calls `sum_to(0)` and asserts `0`.

## When

`sum_to` changes its range from `(0..n).sum()` to `(0..=n).sum()`. Both versions return `0` for `n = 0`, so the only asserted input does not separate the two bounds.

## Then

The finding is not `exposed`: no asserted input distinguishes the old and new range bounds.

## Must Not

Credit the call as exposing the changed range bound because it reaches the changed line (#14419). The honesty corpus independently rejects the false promotion after re-blessing.
