# Fixture: returned_comparison_boundary_gap

Spec: RIPR-SPEC-0108

## Given

One changed threshold, the whole tail expression of a `-> bool` owner:
`ships_free` changes `items > 10` to `items >= 10`. Tests reach it only
through the `shipping(items)` wrapper, with exact assertions at 20 and 2
items. No test uses 10 items.

## When

```bash
cargo xtask fixtures returned_comparison_boundary_gap
```

## Then

- The `return_value` finding for the returned comparison is not `exposed`:
  it names the same missing `items == 10` discriminator the `predicate`
  finding on that line names.

## Must Not

- Promote the returned comparison to `exposed` from inputs (2 and 20) that
  never separate the old and new threshold.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.
