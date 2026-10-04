<!-- section: Fixed -->
- Rust tests marked with `serial_test`'s `#[serial]`, `#[parallel]`,
  `#[file_serial]` or `#[file_parallel]` (bare in a file that names
  `serial_test`, or path-qualified) keep their owner-return pin, so an exact
  `assert_eq!` in them is credited like a plain `#[test]`. Other extra
  attributes still refuse the pin.
- `pretty_assertions::assert_eq!`, `similar_asserts::assert_eq!`,
  `std::assert_eq!` and the other std-shaped assertion macros called by a
  path into those crates are read as assertions with their std oracle kind
  instead of `unknown`. Any other crate path stays unadmitted.
<!-- section: Added -->
- The Rust verdict corpus gains 54 authored cases over 15 small crates that
  test the way projects using common test libraries do: criterion, proptest,
  pretty_assertions, serial_test, mockall, insta, rstest, trybuild,
  expect-test, assert_matches, test-case, quickcheck, tokio::test, claims,
  approx and float-cmp. Each case is labeled with real `cargo test` mutant
  runs against the library from crates.io. No library code is vendored.
