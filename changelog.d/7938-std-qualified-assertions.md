<!-- section: Fixed -->
- A path-qualified standard assertion (`std::assert_eq!`, `::core::assert_eq!`)
  is credited like the bare `assert_eq!` when its root cannot be rebound. It
  used to be refused as an opaque macro, so a correct, strong test read as a
  gap (#7857). A Cargo dependency renamed to `std` or `core`, a rebinding or
  out-of-file glob in the test's file, `#![no_implicit_prelude]`, a
  generated test and other drop-in roots such as `pretty_assertions::` still
  refuse.
