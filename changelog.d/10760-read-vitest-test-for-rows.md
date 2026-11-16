<!-- section: Added -->
- TypeScript: a Vitest `test.for` / `it.for` registration over an inline
  literal table is now indexed with one concrete case per row, so its
  boundary inputs and expected values count like the same `.each` or
  hand-written tests. The whole-row first argument is read through array
  destructuring, a scalar row, or `row[0]`-style reads, and a second-argument
  `{ expect }` context supplies the runner `expect`; other `.for` shapes stay
  disclosed as a partial test extraction (#10760).
