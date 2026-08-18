# Fixture: binding_value_family_matrix

Spec: RIPR-SPEC-0158

## Given

Four changed `let` initializers, one per evaluated std family feeding a
same-function comparison: `strip_prefix` through `map_or`,
`starts_with`, `len`, and `checked_add` through `map_or`. Each function
has an exact-value test whose literal inputs sit exactly on the
predicate boundary.

## When

```bash
cargo xtask fixtures binding_value_family_matrix
```

## Then

Each retargeted predicate finding observes its boundary from the exact
inputs (`infection yes` at the changed boundary; the `x == y` equality
appears among the observed values with the provenance chain retained on
the fact text; no missing-discriminator weakness for the boundary).

## Must Not

- Turn an unevaluated family into an exact value.
- Claim propagation or discrimination the test oracle does not show.
