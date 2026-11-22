# Fixture: typescript_boundary_operand_unicode_parameter

Spec: RIPR-SPEC-0027

## Given

The function directly compares its first parameter `時`. The first argument is exactly the boundary literal.

## When

Analyze the change from `>` to `>=` in `diff.patch`.

## Then

The predicate finding is `exposed` (#15582).

```expected-classes
probe:src_format.ts:typescript_preview:0e5f2be6 -> exposed
```

## Must Not

- Discard a parser-bound parameter because its name uses Unicode.
