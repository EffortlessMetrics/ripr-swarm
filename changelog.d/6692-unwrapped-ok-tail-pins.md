<!-- section: Fixed -->
- A Rust owner that returns `Ok(T { .. })` after an early `return Err(..)`
  now gets whole-value field credit, the same as one that uses `?`. The test
  may compare `owner(..).unwrap()` or `.expect("..")`, inline or through a
  once-used `let`, with the bare `T { .. }` literal. Other early returns,
  `unwrap_or*` and computed `expect` messages still give no credit
  (RIPR-SPEC-0225 decision 6, #6692).
