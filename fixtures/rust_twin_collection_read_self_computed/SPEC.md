# Fixture: rust_twin_collection_read_self_computed

Spec: RIPR-SPEC-0228

## Given

`c.add(20)` and `d.add(20)` both run the owner, then `assert_eq!(c.items, d.items)` compares the two collections (#7135). A wrong push moves both sides together.

## When

`ripr check` analyzes the changed push argument (`x + 1` -> `x`).

## Then

The finding is not `exposed`: neither side is independent of the owner.

## Must Not

Credit the comparison as an observer of the pushed value.
