# Fixture: tier_computed_reversed_mul

Spec: RIPR-SPEC-0197

## Given

A changed low tier (9 to 6) returned through `2 * base` (literal on the
left), pinned by `assert_eq!(tier(4, 30, true), 12)`. Only
`binding * <u32 literal>` is admitted, so the pin earns no witness (#13037).

## When

```bash
cargo xtask fixtures tier_computed_reversed_mul
```

## Then

The changed tier literal stays `static_unknown`.

## Must Not

- Credit a reversed multiplication as the admitted computed shape.
