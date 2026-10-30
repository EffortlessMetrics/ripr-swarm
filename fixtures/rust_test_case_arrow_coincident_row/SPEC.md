# Fixture: rust_test_case_arrow_coincident_row

Spec: RIPR-SPEC-0231
Spec: RIPR-SPEC-0108

## Given

A `#[test_case(0 => 0)]` row whose body returns `scale(x)`. The only row's input is `0`.

## When

`scale` changes from `x * 2` to `x * 3`.

## Then

The finding is `weakly_exposed`: the row reads as `assert_eq!(scale(0), 0)`, which holds under both bodies.

## Must Not

Credit the row as distinguishing the edit. `0 * 2 == 0 * 3`, so no discriminator exists in the row (#10797).
The honesty corpus independently rejects a false promotion after re-blessing.
