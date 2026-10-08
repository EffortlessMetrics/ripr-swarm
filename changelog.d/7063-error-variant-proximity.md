<!-- section: Fixed -->
- A `return Err::<T, E>(E::Variant)` turbofish constructor now reaches the
  error-variant sink like `Err(E::Variant)`, so a test that pins that exact
  variant on the owner's result reads `exposed`. A same-file test that pins the
  same variant on another function's result no longer confirms the owner's
  error path or return value while a test that calls the owner is related (#7063).
- A custom `MyErr::<E>(..)` constructor no longer reads as a `Result::Err`
  construction, and the cross-owner case is pinned in the RIPR-SPEC-0108
  honesty corpus (`rust_shared_error_variant_proximity_other_owner`).
- An `Err(..)` spelled inside a string literal or comment, or after a
  non-ASCII identifier character, no longer reads as an error construction.
