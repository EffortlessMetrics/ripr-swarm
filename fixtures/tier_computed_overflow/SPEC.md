# Fixture: tier_computed_overflow

Spec: RIPR-SPEC-0197

## Given

A changed low tier (3000000000 to 2500000000) returned through `base * 2`,
pinned by `assert_eq!(tier(4, 30, true), 5000000000)`. The product exceeds
`u32::MAX`, so no admitted pin can name it (#13037).

## When

```bash
cargo xtask fixtures tier_computed_overflow
```

## Then

The changed tier literal stays `static_unknown`.

## Must Not

- Credit a computed pin whose product is not representable as `u32`.
