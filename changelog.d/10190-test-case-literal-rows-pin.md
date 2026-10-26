<!-- section: Fixed -->
- A `#[test_case(3, 6)]` function whose body asserts on its parameters
  (`assert_eq!(triangle(n), expected)`) now counts as running that body once
  per row when every row holds one scalar literal per parameter and
  `test_case` is the real crate in the module, so a changed owner it pins
  reads `exposed` instead of `static_unknown` (#10190). The
  `#[test_case(input => expected)]` forms generate a comparison ripr cannot
  read: a changed owner they reach keeps the named
  `rust_macro_wrapped_assertion_unresolved` limit instead of depending on a
  sibling test's unresolved assertion.
