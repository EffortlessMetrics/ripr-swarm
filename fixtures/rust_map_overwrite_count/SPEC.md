# Fixture: rust_map_overwrite_count

Spec: RIPR-SPEC-0228

## Given

The test seeds `stock` with `{A: 1}`, so the key the changed
`insert` writes already exists before the owner runs.

## When

The insert key spelling is rewritten from `sku.to_string()` to the
neutral `sku.to_owned()`, and the test calls `restock("A", 2)` then
asserts `shelf.stock.len() == 1`.

## Then

The insert findings stay `weakly_exposed`: the map holds one entry
whether or not the overwriting insert runs, so the count assertion
cannot discriminate deleting it.

## Must Not

- Credit a `len()` count as confirming an insert that only
  overwrites a pre-existing key.
- Count compilation failure or zero executed tests as evidence.