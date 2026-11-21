# Fixture: typescript_boundary_operand_field

Spec: RIPR-SPEC-0027

## Given

The function compares `o.ms`, initialized from `ms / 2`. The field and first parameter share a name. The first argument is the boundary literal.

## When

Analyze the change from `>` to `>=` in `diff.patch`.

## Then

The predicate finding is `weakly_exposed` (#15582).

```expected-classes
probe:src_format.ts:typescript_preview:5f43a6c0 -> weakly_exposed
```

## Must Not

- Credit the field as the first parameter just because both are named `ms`.
