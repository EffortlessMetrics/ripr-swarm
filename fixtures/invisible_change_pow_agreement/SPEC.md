# Fixture: invisible_change_pow_agreement

Spec: RIPR-SPEC-0197

## Given

`backoff_ms` multiplies by `2u64.pow(attempt)` and the diff changes the base to
`3u64.pow(attempt)`. The only related test asserts `backoff_ms(0, 100) == 100`.

## When

The exact diff is analyzed.

## Then

The return-value finding is not `exposed`: at `attempt = 0` both versions evaluate
to the same value, so the asserted input cannot notice the change (#8659).

## Must Not

- Promote the change to `exposed` from an assertion whose literal input folds equal
  under the changed and the baseline expression.
