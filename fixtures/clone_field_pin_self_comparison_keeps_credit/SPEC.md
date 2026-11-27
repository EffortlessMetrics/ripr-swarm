# Fixture: clone_field_pin_self_comparison_keeps_credit

Spec: RIPR-SPEC-0094

## Given

`Window` has a hand-written `impl Clone` and a derived `PartialEq`. The
changed line is a field of the literal `clone` returns. The test builds a
production `Window` and asserts `assert_eq!(window.clone(), window)`. That
comparison runs the changed `clone` and compares every field with the
original, so it fails against a wrong clone field (#6692).

## When

```bash
ripr check --root fixtures/clone_field_pin_self_comparison_keeps_credit/input \
           --diff fixtures/clone_field_pin_self_comparison_keeps_credit/diff.patch --mode fast
```

## Then

The positive control for the #13037 self-comparison cap. A comparison of a
value with a copy of itself is capped at `weak` everywhere except under the
#6692 clone-field owner pin. Here the pin is admitted, so the finding reads
`exposed` and the related row keeps `exact_value` / `strong`. The shadowed
twins (`owner_return_pin_test_module_shadow` and siblings) run a test-local
clone instead and read `weakly_exposed` with a `weak` row.

The finding's expected class (read against `expected/check.json` by
`check-fixture-contracts`, #12928):

```expected-classes
probe:src_lib.rs:field_construction:721bb1ba -> exposed
```

## Must Not

- Cap the admitted clone-field pin at `weak`.
- Claim runtime mutation adequacy.
