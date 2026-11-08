# Fixture: tier_computed_wrong_multiplier

Spec: RIPR-SPEC-0197

## Given

A changed low tier (9 to 6) returned through `base * 2`, pinned by
`assert_eq!(tier(4, 30, true), 24)`. The pin names neither the current
product (12) nor the previous product (18), so it cannot distinguish the
change (#13037).

## When

```bash
cargo xtask fixtures tier_computed_wrong_multiplier
```

## Then

The changed tier literal stays `static_unknown`; the wrong-multiplier pin
earns no witness.

## Must Not

- Credit a computed pin that matches no admitted product.
- Promote the finding on reach or oracle shape alone.
