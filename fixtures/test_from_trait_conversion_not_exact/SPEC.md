# Fixture: test_from_trait_conversion_not_exact

Spec: RIPR-SPEC-0231

## Given

`score` changes from `points * 3` to `points * 2`. The only test asserts
`assert_eq!(<u8 as From<bool>>::from(score(points) == 0), zero)` over a
table: a comparison converted to an integer through the `From` trait path and
compared with a value that is no boolean literal. It only says whether the
score is zero, so it passes for either body and for most wrong ones.

## When

```bash
cargo xtask fixtures test_from_trait_conversion_not_exact
```

## Then

The related test reads `relational_check` / weak (RIPR-SPEC-0231 rule 1;
#8101), so the finding is not `exposed`.

## Must Not

- Read a comparison carried as an integer through a `From` or `Into` path
  call as an `exact_value` / strong oracle.
- Promote the changed multiplication to `exposed` on that comparison alone.
- Claim runtime mutation adequacy.
