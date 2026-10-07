<!-- section: Fixed -->
- A `return Err::<T, E>(E::Variant)` turbofish constructor now reaches the
  error-variant sink like `Err(E::Variant)`, so a test that pins that exact
  variant on the owner's result reads `exposed`. A same-file test that pins the
  same variant on another function's result no longer confirms the owner's
  error path while a test that calls the owner is related (#7063).
