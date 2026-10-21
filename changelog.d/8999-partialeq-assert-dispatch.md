<!-- section: Changed -->
- A same-file test that compares two values of a type with `assert_eq!` or
  `assert_ne!` now reaches that type's changed user `PartialEq::eq` (or `ne`)
  as a direct owner call, when both operand types are syntactically
  established, no scope shadows the type and no custom `ne` bypasses `eq`.
  The `fixture_opaque` stop reason is now read only from tests that reach the
  owner when any do, instead of from tests that share just a file or a name
  token. No exposure class is raised by either change (#8999).
