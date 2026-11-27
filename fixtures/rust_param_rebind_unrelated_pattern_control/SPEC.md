# Fixture: rust_param_rebind_unrelated_pattern_control

Spec: RIPR-SPEC-0158

## Given

The positive control for the `rust_param_rebind_*` fixtures: the same `m == 40`
comparison with `m = n * 2`, but the destructuring `let (a, _)` binds an
unrelated name, so `n` still holds the caller's value. The test passes `20`.

## When

`ripr check` analyzes the changed comparison (`39` -> `40`).

## Then

The boundary is read from the test input `20`: infection is `yes`, so the
rebinding fixtures' `infection_unknown` comes from the rebinding, not from the
shape of the comparison.

The expected static class per finding (machine-checked against
`expected/check.json` by `check-fixture-contracts`, #12928):

```expected-classes
probe:src_lib.rs:predicate:e1aa58e8 -> exposed
```

## Must Not

Treat an unrelated pattern binding as a rebinding of `n`.
