<!-- section: Fixed -->
- Seam reach no longer reads `opaque` through trait dispatch for a `Debug`,
  `Display`, comparison, `Hash`, `Clone`, `Default`, `FromStr`, serde or
  `Arbitrary` impl just because a test builds the type. The trait's own
  syntax (`{:?}`, `to_string`, `<`, `HashMap`, a serde format crate, a fuzz
  harness, a snapshot macro) must appear in a test, a test file's imports,
  a generic function a test reaches, or test-reached code that names the
  type. A format string in an assertion message does not count, since it runs
  only on failure. On semver, 31 seams in `Debug` and serde impls return to
  `ungripped`, and on bytesize 2 in an `Arbitrary` impl; cargo-mutants missed
  all 23 mutants in those functions. A trait impl for a primitive type
  (`impl Encode for u32`) now reads `opaque` when test-reached code names the
  trait, and a function's own signature no longer counts as a call to its
  name (#5577).
