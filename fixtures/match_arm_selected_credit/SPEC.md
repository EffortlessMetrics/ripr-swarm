# Fixture: match_arm_selected_credit

Spec: RIPR-SPEC-0093

## Given

The same production change as `match_arm_blind`: a `None => 0` arm added to
`match x` in `reason(x: Option<i32>)`. The only test selects that arm and pins
its result exactly:

```rust
assert_eq!(reason(None), 0);
```

`None` has no `::` qualifier, so the variant-token rule of RIPR-SPEC-0093
cannot confirm this arm. Arm selection (#5432, RIPR-SPEC-0229 proposal) reads
the owner call's own argument at the scrutinee position: `None` selects the
`None =>` arm, and the earlier `Some(v) =>` arm provably does not match it.

## When

```bash
cargo xtask fixtures match_arm_selected_credit
```

## Then

The `None =>` probe reads `exposed`: discrimination is confirmed by the
selecting owner call inside the exact assertion.

## Must Not

- Use mutation-runtime outcome vocabulary.
- Credit an arm when the assertion names the variant only on its expected
  side (see `match_arm_expected_side_trap`).
- Credit an arm when an earlier arm with a guard, wildcard or binding could
  take the input first.
