# Fixture: rust_twin_collection_read_literal_control

Spec: RIPR-SPEC-0228

## Given

`c.add(20)` then `assert_eq!(c.items, vec![20])` compares the collection with an independent literal (#7135).

## When

`ripr check` analyzes the changed push argument (`x + 1` -> `x`).

## Then

The finding stays `exposed`.

## Must Not

Demote an exact collection pin.
