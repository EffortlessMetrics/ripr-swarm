# Fixture: test_compared_with_non_literal_not_exact

Spec: RIPR-SPEC-0231

## Given

`score` changes from `points * 3` to `points * 2`. The only test asserts
`assert_eq!(score(points) == 0, expected)` over a table: an equality macro
that compares a comparison with a boolean that is no literal. It only says
whether the score is zero, so it passes for either body and for most wrong
ones.

## When

```bash
cargo xtask fixtures test_compared_with_non_literal_not_exact
```

## Then

The related test reads `relational_check` / weak (RIPR-SPEC-0231 rule 1;
#8006), so the finding is not `exposed`.

## Must Not

- Read a comparison compared with a non-literal bool as an `exact_value` /
  strong oracle.
- Promote the changed multiplication to `exposed` on that comparison alone.
- Claim runtime mutation adequacy.
