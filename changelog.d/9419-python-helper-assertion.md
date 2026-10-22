<!-- section: Fixed -->
- A Python test that asserts through a helper function (`check_equal(f(3), 9)`
  where the helper's body is a single `assert actual == expected`) is no longer
  read as reach-only. A bare-name statement call that resolves to a
  module-level, undecorated, default-free helper whose only statement is an
  `assert` of a comparison over its own parameters is read as that assertion
  with the call's arguments substituted, at the strength the inline form earns.
  Opaque helpers (extra statements, free names, calls, rebound or duplicated
  names, argument-count mismatch) stay unresolved (#9419).
