<!-- section: Fixed -->
- A test with an attribute that cannot change whether or how its body runs
  (`#[rustfmt::skip]`, `#[allow(..)]`, `#[doc = ".."]`, `#[inline]`,
  `#[must_use]`, `#[track_caller]`, a `clippy::` tool attribute) beside `#[test]`
  keeps the owner pin. Before, the extra attribute refused the pinned
  `assert_eq!` and a fully pinned owner read as a `weakly_exposed` gap. An
  import that rebinds the bare attribute name, `#[ignore]`, `#[should_panic]`,
  `cfg` and unknown attributes still refuse (#9439).
