# Fixture: rust_deref_receiver_alias_std_trait

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The inherent `Inner::len` changes. `Guard` derefs to `Inner`, and `type G = Guard;` carries `impl Iterator for G` and `impl ExactSizeIterator for G`, so `Guard` has a std `len(&self)`. The test asserts `g.len() == 0` on `let g = Guard(Inner { cap: 3 });`. It passes on the old code and on the new code, because `ExactSizeIterator::len` on `&Guard` is found before the deref.

## When

`cargo xtask fixtures rust_deref_receiver_alias_std_trait` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed`. A `type` item that names the wrapper refuses the deref reading, since an impl on the alias is an impl on the wrapper.

## Must Not

- Credit `Inner::len` because no `impl` header spells `Guard`.
