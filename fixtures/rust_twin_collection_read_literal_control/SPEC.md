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

## Declared classes

```expected-classes
[
  [
    "probe:src_lib.rs:call_deletion:3c28bb05",
    "exposed"
  ],
  [
    "probe:src_lib.rs:side_effect:fe1fff37",
    "exposed"
  ]
]
```
