# Fixture: rust_map_insert_count_empty

Spec: RIPR-SPEC-0228

## Given

The test starts from an empty map: `Shelf::new()` holds no entries
before the owner runs.

## When

The insert key spelling is rewritten from `sku.to_string()` to the
neutral `sku.to_owned()`, and the test calls `restock("A", 2)` then
asserts `shelf.stock.len() == 1`.

## Then

The insert findings stay `exposed`: without the insert the map is
empty, so the count assertion discriminates deleting it. This is
the positive control for `rust_map_overwrite_count`.

## Must Not

- Downgrade an empty-start insert confirmed by its count.
- Count compilation failure or zero executed tests as evidence.