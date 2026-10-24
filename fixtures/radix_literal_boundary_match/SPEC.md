# Fixture: radix_literal_boundary_match

Spec: RIPR-SPEC-0001

Issue: #9164

## Given

The changed predicate is `n > 0xFF` (was `n >= 0xFF`). The related test calls
`is_big(0xFF)`, spelled the same as the boundary.

## When

```bash
cargo xtask fixtures radix_literal_boundary_match
```

## Then

The predicate finding stays `exposed`: the same spelled radix literal is the
boundary input. This is the control for `radix_literal_boundary_off`.

## Must Not

- Lose the boundary credit for a matching radix-prefixed literal.
