# Fixture: rust_slice_counter_early_error_equality

Spec: RIPR-SPEC-0186. Honesty contract: RIPR-SPEC-0108. Issue: #5346.

## Given

Both repeated inputs contain an invalid hex byte. The conversion returns
BadDigit before reaching the counter boundary. Custom error equality makes
the long assertion's spelled TooLong equal to that actual BadDigit result.

## When

The predicate changes from `count > 17` to `count >= 17`.

## Then

The predicate stays below exposed. Native assertions pass under both predicates;
derived equality rejects the long assertion. Valid hex bytes retain the genuine
counter-boundary positive. A rebound loop byte cannot borrow the input's validity.
