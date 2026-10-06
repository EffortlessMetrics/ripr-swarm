<!-- section: Fixed -->
- Rust: a test that matches the owner's `Result` with an Ok arm that panics
  and an Err arm that is one `assert_eq!(e, Type::Variant)` (or
  `assert!(matches!(e, Type::Variant))`) now credits that exact error. A
  quiet Ok arm, a guard, or a catch-all arm still gets no credit (#6673).
- Rust: a changed `helper(..).ok_or(Type::Variant)?` (or
  `.ok_or_else(|| Type::Variant)?`) in the owner's body now counts as
  returning that error from the owner. A test that pins
  `Err(Type::Variant)` on the owner call reads `exposed`. A test that pins a
  different variant gets no credit, and a `?` inside a closure or `async`
  block is not counted (#6695).
- Rust: an error variant whose name contains the letters `ffi` (such as
  `Insufficient`) is no longer treated as an FFI boundary that blocks its
  propagation evidence (#6673).
- Rust: a changed `return Err::<T, E>(Type::Variant)` now binds its exact
  variant like `Err(Type::Variant)`. A test that pins a different variant
  of the same error no longer makes the error-path finding read `exposed`
  (#6673).
- Rust: an assertion that names only a sibling variant of the changed
  error's enum (`assert!(matches!(e, PayError::Limit))` against a changed
  `Err(PayError::Insufficient)`) no longer makes the error-path finding read
  `exposed` through the shared enum name (#6673).
- Rust: in repository exposure mode, an error seam built from
  `return x.ok_or(Type::Variant)?` or a tail `x.ok_or(Type::Variant)?`
  now carries that variant, so only a test that pins that variant credits
  the seam (#6695).
