# Fixture: inequality_struct_assertion_not_exact

Spec: RIPR-SPEC-0231

## Given

`build` changes the `retries` field from `retries * 2` to `retries + 1`. The only
test asserts `assert_ne!(build(3), Config { retries: 9 })`, which passes for
either body and for every other wrong value except 9.

## When

```bash
cargo xtask fixtures inequality_struct_assertion_not_exact
```

## Then

The related test keeps the `whole_object_equality` kind at weak strength
(RIPR-SPEC-0231 rule 1, acceptance example 14), and no probe family lifts it
to the strong credit that kind earns from `assert_eq!`. The finding is not
`exposed`.

## Must Not

- Report the struct inequality as a strong oracle for the changed field.
- Promote the changed field to `exposed` on an inequality alone.
- Claim runtime mutation adequacy.
