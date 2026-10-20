# Fixture: owner_pin_self_computed_holder_not_a_pin

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #8928

## Given

A changed owner `weight` is compared in two tests with an expected value the
test computes by calling `weight` itself (`let expected: u32 = weight(4);`).
The test module also holds a helper `macro_rules!` and a cfg-gated
`struct Counter(AtomicUsize)` shadowed by a plain `struct Counter(u32)`.

## When

The public analyzer consumes the diff and the current tests.

## Then

The owner-pin does not admit either assertion as an independent expected value;
the finding is not promoted. A refusal to read the binding for owner-pin
admission never clears the self-computed cap.

## Must Not

- Promote the finding to `exposed` because a `macro_rules!` or a cfg-gated
  holder type made the expected binding unreadable.
