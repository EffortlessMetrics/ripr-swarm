<!-- section: Fixed -->
- Verdicts: a private macro binding in another crate of the same package no
  longer refuses every `assert_eq!`. A bench's `#[macro_use] extern crate
  bencher;` (humantime), a private glob or import, `#![no_implicit_prelude]`
  or a non-exported `macro_rules! assert_eq` in `benches/`, `examples/`,
  `build.rs` or another target root now reaches only tests compiled in that
  crate. Exported or re-exported bindings, and files ripr cannot place in a
  recognized target, still reach every test (#5352).
- Verdicts: an exact pin through an inline receiver
  (`assert_eq!(Stack::new(1).depth(), 1)`) is typed like a `let` binding, and
  a changed early `return None;` / `return Err(..);` is pinned when it is the
  only source of that value and every other exit builds `Some(..)` / `Ok(..)`.
  The verdict corpus cases `bytesize-as-kib-div` and `bytesize-as-mb-div`
  now read `exposed` (#5352).
