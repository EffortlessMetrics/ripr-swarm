# Fixture: rust_qualified_foreign_receiver

Spec: RIPR-SPEC-0005

Honesty contract: RIPR-SPEC-0108. Issue: #8782, item 1.

## Given

Two workspace crates export `WhileSome` and implement `Iterator::size_hint`.
Only the local crate's return changes. The test constructs
`foreign::WhileSome` and pins that other crate's size hint with strong exact
assertions. A second local `OtherIterator::size_hint` ensures Fast mode indexes
competing methods even though it omits the foreign dependency. Name uniqueness
therefore cannot stand in for receiver identity in this fixture.

## When

`ripr check` analyzes the local `size_hint` return change.

## Then

The finding is never promoted to `exposed` and the test relation stays
`weak_token_substring`. Dropping the `foreign::` prefix cannot establish a
direct call to the local owner's type. Since gap withholding landed on main
the finding is reported as `static_unknown` with stop reason
`gap_evidence_unresolved` (the implicit iterator-call gap) rather than
`weakly_exposed`. The class pin holds only non-promotion; the
`weak_token_substring` relation pins are what reject the old reach claim. The honesty corpus independently pins
the nonempty finding, non-promotion and exact related-test identity.

The existing `rust_same_method_owner_type_positive` fixture retains the
matching bare owner receiver and its `exposed` classification.

## Boundary

This fences qualified receiver initializers and type annotations in the
compact-name matcher. Direct UFCS, unit-value paths and borrowed helper-call
context remain separate from this slice, as do unique-name inherent
attribution and broader nominal receiver resolution under #7415/#8577.
Constructor return types remain #8782 item 2. No general type resolution,
cross-crate identity proof or runtime adequacy claim is added.

## Must Not

- Attribute either foreign receiver to the changed local `WhileSome` method.
- Promote the finding through strong assertions on that foreign method.
- Treat the local competing method as evidence that the dependency was indexed.
