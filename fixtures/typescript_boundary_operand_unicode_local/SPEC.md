# Fixture: typescript_boundary_operand_unicode_local

Spec: RIPR-SPEC-0027

## Given

The local `時` is half the first argument. The unused second argument is the boundary literal.

## When

Analyze the change from `>` to `>=` in `diff.patch`.

## Then

The predicate finding is `weakly_exposed` (#15582).

```expected-classes
probe:src_format.ts:typescript_preview:0e5f2be6 -> weakly_exposed
```

## Must Not

- Credit an unrelated literal for a Unicode local.
