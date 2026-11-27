# Fixture: rust_deref_receiver_wrapper_method

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The inherent `Inner::limit` changes. `Guard` derefs to `Inner`, but `impl Guard` also has its own `fn limit(&self) -> u32 { 100 }`. The test asserts `Guard(Inner { cap: 3 }).limit() == 100` through a `let`. It passes on the old code and on the new code, because method lookup takes `Guard::limit` before it derefs.

## When

`cargo xtask fixtures rust_deref_receiver_wrapper_method` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed`. A second `fn limit` in the workspace refuses the deref reading.

## Must Not

- Credit `Inner::limit` because `Guard` derefs to `Inner`.
