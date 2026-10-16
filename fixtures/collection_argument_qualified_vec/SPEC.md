# Fixture: collection_argument_qualified_vec

Spec: RIPR-SPEC-0094

## Given

The push is rewritten from `x` to neutral `x * 1`. The observed field is an explicitly qualified standard vector.

## When

The actual public check reads the fixture and independent literal equality.

## Then

Both collection effects read `exposed`. Neutral compiles and passes. Wrong-value compiles and fails; deleted push compiles and fails the intended equality.

## Must Not

- Treat compilation failure or an unexecuted test as proof.
- Infer asserted field equality from a coerced callee parameter or constructor spelling.
