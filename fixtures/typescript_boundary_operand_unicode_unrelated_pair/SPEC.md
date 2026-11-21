# Fixture: typescript_boundary_operand_unicode_unrelated_pair

Spec: RIPR-SPEC-0027

## Given

The unused second argument equals the first. The compared parameters `時` and `limit` receive zero and ten. Both operators take the same branch.

## When

Analyze the change from `>` to `>=` in `diff.patch`.

## Then

The predicate finding is `weakly_exposed` (#15582).

```expected-classes
probe:src_format.ts:typescript_preview:b7991e98 -> weakly_exposed
```

## Must Not

- Credit the duplicate in the unused position.
