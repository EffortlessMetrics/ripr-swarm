# Fixture: slice elements are not cardinality

Spec: RIPR-SPEC-0001

## Given

The same retained grid subject and edit as `rust_slice_table_boundary`, with
nonempty literal input slices `[0, 0]` and `[0, 0, 0]` in its property table.

## When

`ripr check` analyzes `samples.len() == 0`.

## Then

The finding is `weakly_exposed` and names the missing empty-slice input with
observed lengths 2 and 3.

## Must Not

Credit an empty-boundary equality from the zero-valued elements or unrelated
assertion expectations.
