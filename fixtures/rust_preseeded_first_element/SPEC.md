# Fixture: rust_preseeded_first_element

Spec: RIPR-SPEC-0228

## Given

`Ledger::seeded` builds `log` as `vec![9]`, so the collection already
holds an element before the changed push runs.

## When

The push is rewritten from `x` to the neutral `x * 1`, and the test
calls `record(7)` then asserts `ledger.log.first() == Some(&9)`.

## Then

The push findings stay `weakly_exposed`: the first element is the
pre-seeded `9` whether or not the push happens or what it pushes, so
the assertion cannot discriminate the changed push.

## Must Not

- Credit a first-element read of a pre-seeded collection as observing
  the pushed element.
- Count compilation failure or zero executed tests as evidence.
