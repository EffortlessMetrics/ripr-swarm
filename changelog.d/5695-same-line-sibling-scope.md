<!-- section: Fixed -->
- Same-line sibling `mod` scopes no longer taint a neighboring `#[test]`'s
  `assert_eq!`, while a borrowed oracle from a one-line check helper with its
  own scoped `assert_eq!` rebinding stays refused (#5695).
