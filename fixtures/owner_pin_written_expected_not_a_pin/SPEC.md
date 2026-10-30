# Fixture: owner_pin_written_expected_not_a_pin

Spec: RIPR-SPEC-0108

Owner: analysis-fixtures

Issue: #8795

## Given

A changed owner `weight` is compared in two tests with an expected value the
test itself writes the owner result into: through a shared borrow of a struct
that `Deref`s to an atomic, and through a keyword-led array destructuring
assignment (`match [expected, other] = [weight(4), 0]`).

## When

The public analyzer consumes the diff and the current tests.

## Then

The owner-pin does not admit either assertion as an independent expected value;
the finding is not promoted.

## Must Not

- Promote the finding to `exposed` through either self-derived expected value.
- Treat a struct literal of literals as free of interior mutability.
- Read a `[` after a keyword as an index.
