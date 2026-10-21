# Fixture: returned_explicit_comparison_boundary_gap

Spec: RIPR-SPEC-0108

## Given

One changed threshold, the explicit `return` of a `-> bool` owner:
`ships_free` changes `return items > 10;` to `return items >= 10;`. Tests reach
it only through the `shipping(items)` wrapper, with exact assertions at 20 and
2 items. No test uses 10 items.

## When

```bash
cargo xtask fixtures returned_explicit_comparison_boundary_gap
```

## Then

- The `return_value` finding for the explicitly returned comparison is not
  `exposed`: it names the same missing `items == 10` discriminator the
  `predicate` finding on that line names, as the tail form does (#9116).

## Must Not

- Promote the returned comparison to `exposed` from inputs (2 and 20) that
  never separate the old and new threshold because of a leading `return`.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.
