# Fixture: rust_pushed_element_last

Spec: RIPR-SPEC-0228

## Given

`Ledger::seeded` builds `log` as `vec![9]`, so the collection already
holds an element before the changed push runs.

## When

The push is rewritten from `x` to the neutral `x * 1`, and the test
calls `record(7)` then asserts `ledger.log.last() == Some(&7)`.

## Then

The push findings read `exposed`: the last element is exactly the
pushed element, so the assertion discriminates the changed push.

## Must Not

- Demote a read of the pushed (last) element merely because the
  collection was pre-seeded.
- Count compilation failure or zero executed tests as evidence.
