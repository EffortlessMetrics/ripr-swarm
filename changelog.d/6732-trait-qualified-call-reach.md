<!-- section: Fixed -->
- A test that calls a trait method through its trait path,
  `Render::render(&-0.0f64)` or `<f64 as Render>::render(&z)`, now
  relates directly to the impl that its first argument selects. Before,
  when another `render` existed, it read as a name-only match with "No
  test is seen calling render". The argument must name its type by its
  own syntax: a suffixed literal, a typed `let` binding, or a
  constructor. Otherwise the call stays name-only (#6732).
