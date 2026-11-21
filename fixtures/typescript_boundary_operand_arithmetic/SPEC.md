# Fixture: typescript_boundary_operand_arithmetic

Spec: RIPR-SPEC-0027

## Given

The first argument is 5 and the unused second argument is the boundary literal. The function compares `ms / 2`.

## When

Analyze the change from `>` to `>=` in `diff.patch`.

## Then

The predicate finding is `weakly_exposed` (#15582).

```expected-classes
probe:src_format.ts:typescript_preview:5f58bae9 -> weakly_exposed
```

## Must Not

- Treat a fragment of the arithmetic expression as the compared input.
