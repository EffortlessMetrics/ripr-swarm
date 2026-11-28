# Fixture: collection_push_contents_check_next_step

Spec: RIPR-SPEC-0412

## Given

`log_line` is a free function whose only statement is one unconditional push
into its `&mut Vec<String>` parameter (the owner shape the #11402 repair route
follows). The test passes a fresh local and reads it back only by
`assert_eq!(buf.len(), 1)`.

## When

The pushed value is rewritten to the neutral `msg.to_string() + ""`.

## Then

The side-effect finding is `weakly_exposed` and its recommended next step asks
for an exact check of the full contents the test reads back, not a mock
expectation (#12456). A conditional push, aliased, wrong or unrelated receiver
keeps the mock-expectation wording (see `rust_direct_collection_count_payload`
and the `collection_argument_*` fixtures).

## Must Not

- Suggest a mock expectation for this seam.
- Change the class or any repair signal.
