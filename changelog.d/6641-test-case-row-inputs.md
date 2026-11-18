<!-- section: Fixed -->
- Rust: a `#[test_case(..)]` row now counts as the test's input, as an rstest
  `#[case(..)]` row already did, so a row at a changed boundary
  (`#[test_case(10, 90)]`, `#[test_case(10 => 90)]`, or a named
  `#[test_case(10, 90 ; "ten")]`) reads `exposed` instead of
  `weakly_exposed`. Ignored rows, `=> panics`/`matches`/`with` rows and
  `#[should_panic]` functions bind no inputs (#6641).
