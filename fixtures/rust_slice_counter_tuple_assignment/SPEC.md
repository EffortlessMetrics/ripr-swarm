# Fixture: rust_slice_counter_tuple_assignment

Spec: RIPR-SPEC-0186. Honesty contract: RIPR-SPEC-0108. Issue: #5346.

## Given

A tuple assignment changes count after the check, skipping the adjacent boundary. Both predicates pass the assertions; removing the assignment makes the wrong predicate fail.

## When

The counter comparison changes from `>` to `>=`.

## Then

The predicate stays below exposed. Ordinary unchanged counters and representable boundary values retain their positive credit.
