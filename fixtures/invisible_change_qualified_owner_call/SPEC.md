# Fixture: invisible_change_qualified_owner_call

Spec: RIPR-SPEC-0197

## Given

`backoff_ms` multiplies by `2u64.pow(attempt)` and the diff changes the base to
`3u64.pow(attempt)`. The only related test calls the owner through its crate path
and asserts `invisible_change_qualified_owner_call::backoff_ms(0, 100) == 100`.

## When

The exact diff is analyzed.

## Then

The return-value finding is not `exposed`: at `attempt = 0` both versions evaluate
to the same value, so the asserted input cannot notice the change. The
path-qualified call reads like the bare call in `invisible_change_pow_agreement`
(#15414).

## Must Not

- Promote the change to `exposed` because the asserted owner call is spelled
  through a crate path.
