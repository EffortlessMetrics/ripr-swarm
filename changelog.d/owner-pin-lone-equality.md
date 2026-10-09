<!-- section: Fixed -->
- The owner-return pin now reads the compared operands of a plain
  `assert!(owner(x) == value)` and of the assertion twin of a terminal
  Err-return guard `if owner(x) != value { return Err(..) }`, so these
  tests pin a changed return value as `assert_eq!(owner(x), value)` does.
  Only a lone top-level `==` counts: an `==` guard, a negated condition,
  `<`/`>`/`<=`/`>=`/`!=`, an equality joined by `&&` or `||`, and an
  expected side that names or reaches the owner stay below `exposed`. The
  verdict-corpus case `checkout-fee-err-return-guard` now reads credited
  (RIPR-SPEC-0197, #7063).
- An Err-return guard in a file that binds the value name `Err` (a `fn`,
  `const`, `static` or struct constructor, a pattern binding or parameter,
  an import of the name, or any glob import) stays below `exposed`: the
  shadowed `Err` can return `Ok` on the changed behavior, so the guard is
  not the twin of `assert_eq!` in outcome. The refusal is file-wide and
  pinned by `fixtures/owner_return_pin_err_guard_shadowed_err`
  (#7063 review).
