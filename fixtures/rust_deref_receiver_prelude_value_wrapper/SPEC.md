# Fixture: rust_deref_receiver_prelude_value_wrapper

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The inherent `Inner::is_some` changes its return value from `true` to `false`. The crate root declares a tuple struct `Some` that derefs to `Inner`. The test module has no glob import, so its `Some(crate::Inner { cap: 1 })` builds a prelude `Option`, and `.is_some()` is `Option::is_some`. The test passes on the old code and on the new code.

## When

`cargo xtask fixtures rust_deref_receiver_prelude_value_wrapper` runs the public diff analyzer.

## Then

The finding does not read `exposed` or `weakly_exposed`. A wrapper named like a std prelude value refuses the deref reading.

## Must Not

- Credit `Inner::is_some` because a workspace `Some` derefs to `Inner`.
