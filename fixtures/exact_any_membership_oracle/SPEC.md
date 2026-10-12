# Fixture: exact_any_membership_oracle

Spec: RIPR-SPEC-0231

## Given

Production code changes the scaled amount in `audit_line` from `amount * 7`
to `amount * 8`, and the related test requires `"audited 42"` via exact
`.any()` membership over the collected lines.

## When

```bash
cargo xtask fixtures exact_any_membership_oracle
```

or:

```bash
ripr check --root fixtures/exact_any_membership_oracle/input --diff fixtures/exact_any_membership_oracle/diff.patch --mode fast
```

## Then

`ripr` should report the related test's oracle as `exact_value` / strong:
the assertion pins one exact member value, so any wrong value fails it.

## Must Not

- Use mutation-runtime outcome vocabulary reserved for real mutation execution.
- Degrade the exact membership assertion into a broad, smoke, or weak oracle.
