# Fixture: seeded collection expectation after an early return

Spec: RIPR-SPEC-0228

## Given

`Pulse::beat` returns early when `locked` is set, before
`self.log.push(1)`. The only test seeds `log = vec![9]`, locks the
owner, calls `beat()`, and asserts `p.log == vec![9]`.

## When

`ripr check` analyzes the `push(1)` to `push(2 - 1)` edit.

## Then

Both push findings stay `weakly_exposed`: the assertion only restates
the seeded collection, so it passes whether the push is deleted,
changed, or never runs (#8205, extending #8051).

## Must Not

- Credit the seeded expectation as a strong oracle for the guarded push.
- Weaken the #8051 empty-collection behavior or the unguarded
  seeded-plus-pushed control (`vec![9, 1]` stays `exposed`).
