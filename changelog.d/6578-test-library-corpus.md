<!-- section: Added -->
- The Rust verdict corpus gains 54 authored cases over 15 small crates that
  test the way projects using common test libraries do: criterion, proptest,
  pretty_assertions, serial_test, mockall, insta, rstest, trybuild,
  expect-test, assert_matches, test-case, quickcheck, tokio::test, claims,
  approx and float-cmp. Each case is labeled with real `cargo test` mutant
  runs against the library from crates.io. No library code is vendored
  (#6578).
