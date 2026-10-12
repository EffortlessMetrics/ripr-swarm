<!-- section: Fixed -->
- A test that calls a trait method through its trait path now relates
  directly to the impl that its first argument selects. This covers
  `Render::render(&-0.0f64)` and `<f64 as Render>::render(&z)`. Before,
  when another `render` existed, such a test read as a name-only match,
  with "No test is seen calling render". The new relation needs three
  things: the owner takes `self`, its trait is not generic, and the
  argument names its type by its own syntax. That syntax can be a
  suffixed literal, a typed `let` binding, or a struct, tuple-struct
  or variant literal. Any other call stays name-only, including one
  whose argument is an associated function such as `Site::new()`
  (#6732).
