# Fixture: rust_deref_receiver_private_method

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The private inherent `Inner::len`, declared inside the sibling module `inherent`, changes from `cap * 2` to `cap * 2 + 1`. `Guard` derefs to `Inner`, and `Inner` implements `Iterator` with a size hint of `(7, Some(7))` and `ExactSizeIterator` without overriding `len`. The test module cannot see the private method, so its `g.len()` resolves to the prelude `ExactSizeIterator::len`. The test passes on the old code and on the new code.

## When

`cargo xtask fixtures rust_deref_receiver_private_method` runs the public diff analyzer.

## Then

The finding does not read `exposed` or `weakly_exposed`. Only a method declared with a plain `pub` may take the deref reading.

## Must Not

- Credit the private `Inner::len` because `Guard` derefs to `Inner`.
