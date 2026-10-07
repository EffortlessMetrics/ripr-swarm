<!-- section: Fixed -->
- Rust: a test that calls a method on a suffixed numeric literal, such as
  `(-0.0f64).render()` or `1.5f64.render()`, now relates to that method in
  `impl Render for f64` as a direct owner call. Before, ripr reported "No test
  is seen calling render" (#7019, #6732).
