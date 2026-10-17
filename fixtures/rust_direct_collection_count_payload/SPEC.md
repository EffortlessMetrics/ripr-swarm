# Fixture: rust_direct_collection_count_payload

Spec: RIPR-SPEC-0231

## Given

`send` pushes `id` onto the caller-owned `events_sent` collection
(direct collection argument, #6640 / spec0231 example 9).

## When

The push is rewritten from `id` to the neutral `0 + id`, and the test
calls `send(&mut events_sent, 7)` then asserts
`events_sent.len() == 1`.

## Then

The side-effect (payload) finding stays `weakly_exposed`: the count
pins how many elements were pushed, not which value, so a
`push(id + 1)` mutant passes it. The call-deletion finding stays
`exposed`: deleting the push changes the count the assertion pins.

## Must Not

- Credit a pure count read (`.len()`, `.is_empty()`) as observing the
  pushed payload for a value or side-effect probe.
- Demote the call-deletion probe that the same count read does
  discriminate.
- Count compilation failure or zero executed tests as evidence.
