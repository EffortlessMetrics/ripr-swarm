<!-- section: Fixed -->
- The owner-return pin now reads the compared operands of a plain
  `assert!(owner(x) == value)` and of the assertion twin of a terminal
  Err-return guard `if owner(x) != value { return Err(..) }`, so these
  tests pin a changed return value as `assert_eq!(owner(x), value)` does.
  Only a lone top-level `==` counts: an `==` guard, a negated condition,
  `<`/`>`/`<=`/`>=`/`!=`, an equality joined by `&&` or `||`, and an
  expected side that names or reaches the owner stay below `exposed`. The
  verdict-corpus case `checkout-fee-err-return-guard` now reads credited.
