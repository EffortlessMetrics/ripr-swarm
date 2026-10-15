# Fixture: test_cast_to_integer_not_exact

Spec: RIPR-SPEC-0231

## Given

`score` changes from `points * 3` to `points * 2`. The only test asserts
`assert_eq!(u8::from(score(points) == 0), zero)` over a table: a comparison
converted to an integer and compared with a value that is no boolean literal.
It only says whether the score is zero, so it passes for either body and for
most wrong ones.

## When

```bash
cargo xtask fixtures test_cast_to_integer_not_exact
```

## Then

The related test reads `relational_check` / weak (RIPR-SPEC-0231 rule 1;
#8093), so the finding is not `exposed`.

## Must Not

- Read a comparison carried as an integer through `From` or a cast as an
  `exact_value` / strong oracle.
- Promote the changed multiplication to `exposed` on that comparison alone.
- Claim runtime mutation adequacy.
