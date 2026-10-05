- A test of a same-named function in another module no longer counts as a
  test of the changed function. `b::render(..)`, or a bare `delay(..)` that
  `use super::*` binds to a sibling file's own `delay`, now supplies no reach
  and no confirming tokens. This removes a false `exposed` (#6537) and a
  false grip (#6292).
- A `use` import now settles which same-named function a bare call names,
  so `use super::celsius::snap;` with an exact `assert_eq!` reads `exposed`
  instead of a false `weakly_exposed` (#6544).
