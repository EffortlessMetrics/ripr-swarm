<!-- section: Changed -->
- A same-file test that compares two values of a type with `assert_eq!` or
  `assert_ne!` now reaches that type's changed user `PartialEq::eq` (or `ne`)
  as a direct owner call, when both operand types are syntactically
  established, no scope shadows the type and no custom `ne` bypasses `eq`.
  Files with a `use` that may name another `T`, a user `assert_eq` macro or
  pattern-rebound operands are not credited. For a `PartialEq::eq`/`ne` owner
  only, the `fixture_opaque` stop reason is read from tests that reach the
  owner when any do, instead of from tests that share just a file or a name
  token. Exposure classes are unchanged versus an explicit `a.eq(&b)` call
  (#8999).
