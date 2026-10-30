# Fixture: test_in_struct_literal_not_exact

Spec: RIPR-SPEC-0231

## Given

`score` changes from `points * 3` to `points * 2`. The only test asserts
`assert_eq!(Check { zero: score(points) == 0 }, Check { zero })` over a table:
a comparison held in a struct literal and compared with a struct built from a
value that is no boolean literal. It only says whether the score is zero, so
it passes for either body and for most wrong ones.

## When

```bash
cargo xtask fixtures test_in_struct_literal_not_exact
```

## Then

The related test reads `relational_check` / weak (RIPR-SPEC-0231 rule 1;
#8615), so the finding is not `exposed`.

## Must Not

- Read a comparison held in a struct literal, compared with a non-literal, as
  a whole-object or `exact_value` / strong oracle.
- Promote the changed multiplication to `exposed` on that comparison alone.
- Claim runtime mutation adequacy.
