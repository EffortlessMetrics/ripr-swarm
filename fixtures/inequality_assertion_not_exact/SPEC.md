# Fixture: inequality_assertion_not_exact

Spec: RIPR-SPEC-0231

## Given

`score` changes from `points * 3` to `points * 2`. The only test asserts
`assert_ne!(score(2), 0)`, which passes for either body and for most wrong
ones.

## When

```bash
cargo xtask fixtures inequality_assertion_not_exact
```

## Then

The related test reads `relational_check` / weak (RIPR-SPEC-0231 rule 1,
acceptance example 1), so the finding is not `exposed`.

## Must Not

- Read `assert_ne!` as an `exact_value` / strong oracle.
- Promote the changed multiplication to `exposed` on an inequality alone.
- Claim runtime mutation adequacy.
