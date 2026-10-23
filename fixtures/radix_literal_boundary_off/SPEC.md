# Fixture: radix_literal_boundary_off

Spec: RIPR-SPEC-0001

Issue: #9164

## Given

The changed predicate is `n > 0xFF` (was `n >= 0xFF`). The only related test
calls `is_big(0x10)`, a hex input far from the `0xFF` boundary.

## When

```bash
cargo xtask fixtures radix_literal_boundary_off
```

## Then

The predicate finding is `weakly_exposed` and names the missing `n == 0xFF`
boundary input. A radix-prefixed integer literal is one spelled value, so
`0x10` is not the boundary.

## Must Not

- Read `0x10` or `0xFF` as the value `0` and report the boundary as observed.
- Report `exposed` for an input that is not at the changed boundary.
