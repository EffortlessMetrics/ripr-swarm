# Fixture: rust_range_bound_two_ranges_one_line

Spec: RIPR-SPEC-0108

## Given

A test that calls `both(0, 0)` and asserts `0`.

## When

`both` changes both ranges on one line from `(0..a).sum() + (0..b).sum()` to `(0..=a).sum() + (0..=b).sum()`. Both versions return `0` at `a = b = 0`, so the only asserted input does not separate the old and new bounds.

## Then

The finding is not `exposed`: no asserted input that the test runs distinguishes the old and new range bounds.

## Must Not

Credit the call as exposing the changed range bounds because the line holds more than one range (#14538). The honesty corpus independently rejects the false promotion after re-blessing.
