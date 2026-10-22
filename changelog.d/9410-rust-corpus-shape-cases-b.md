<!-- section: Internal / CI -->
- The Rust verdict corpus gains 20 more language-shape cases in
  `authored-shape-kit` (operator, `From`, `FromStr`, `Iterator`, `Default` and
  `Drop` impls, `dyn` dispatch, generics, `impl Trait`, closures, labeled
  `break`, async with a hand-written executor, `Option::map_or`), and the
  `language_shape` floor rises from 13 to 33. Labels come from replayed
  mutants (20 cases, 0 drifted). ripr reads 9 ideally, abstains on 4 and gives
  7 false actionable verdicts on this base; each is recorded in its blessed row
  and filed under #9410.
