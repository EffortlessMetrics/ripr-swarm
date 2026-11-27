# Fixture: rust_deref_receiver_value_shadow

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The inherent `Inner::len` changes. `Guard` is a named-field struct that derefs to `Inner`, and a `const Guard: fn(Inner) -> [u8; 7]` shares its name in the value namespace. The test asserts `Guard(Inner { cap: 3 }).len() == 7`. `Guard(..)` calls the const, so `.len()` is the array's length. The test passes on the old code and on the new code.

## When

`cargo xtask fixtures rust_deref_receiver_value_shadow` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed`. A value item named like the wrapper refuses the deref reading.

## Must Not

- Credit `Inner::len` because `Guard` derefs to `Inner`.
