# Fixture: owner_pin_written_alias_not_a_pin

Spec: RIPR-SPEC-0108

Owner: analysis-fixtures

Issue: #8944

## Given

A changed owner `weight` is compared in three tests with an expected static the
test itself writes the owner result into: through a `ref mut` binding in a tuple
`match` scrutinee, and through the same binding when a struct literal with a
brace body follows the name in the tuple, and when the name sits two groups deep with the braced sibling after the outer group.

## When

The public analyzer consumes the diff and the current tests.

## Then

The owner-pin does not admit either assertion as an independent expected value;
the finding is not promoted.

## Must Not

- Promote the finding to `exposed` through either self-derived expected value.
- Read a tuple `ref mut` alias as a read of the static.
- Take a brace inside the scrutinee tuple, at any nesting depth, for the `match` arm list.

The golden and fixture checks bind this behavior (they flip the finding to
`exposed` if the nested-brace scan regresses). The honesty-corpus case
`rust_owner_pin_written_alias_not_a_pin` is a smoke guard only: it does not
discriminate the brace scan.
