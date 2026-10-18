# Fixture: test_wrapped_in_some_not_exact

Spec: RIPR-SPEC-0231

## Given

`score` changes from `points * 3` to `points * 2`. The only test asserts
`assert_eq!(Some(score(points) == 0), Some(zero))` over a table: a comparison
wrapped in `Some` and compared with a wrapped value that is no boolean
literal. It only says whether the score is zero, so it passes for either body
and for most wrong ones.

## When

```bash
cargo xtask fixtures test_wrapped_in_some_not_exact
```

## Then

The related test reads `relational_check` / weak (RIPR-SPEC-0231 rule 1;
#8101), so the finding is not `exposed`.

## Must Not

- Read a comparison held in a value wrapper (`Some`, `Ok`, a constructor,
  `Box::new`, an array or `vec!`) as an `exact_value` / strong oracle.
- Promote the changed multiplication to `exposed` on that comparison alone.
- Claim runtime mutation adequacy.
