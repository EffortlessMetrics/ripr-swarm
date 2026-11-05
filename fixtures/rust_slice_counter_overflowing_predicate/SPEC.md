# Fixture: rust_slice_counter_overflowing_predicate

Spec: RIPR-SPEC-0186. Honesty contract: RIPR-SPEC-0108. Issue: #5346.

## Given

The u8 predicate literal 256 truncates under overflowing_literals. Both predicates pass through error equality although a usize counter makes the wrong predicate fail.

## When

The counter comparison changes from `>` to `>=`.

## Then

The predicate stays below exposed. Ordinary unchanged counters and representable boundary values retain their positive credit.
