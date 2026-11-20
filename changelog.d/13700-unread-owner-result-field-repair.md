<!-- section: Fixed -->
- `ripr repair` now starts the field-value route when the only related test
  binds the owner's result and never reads the changed field (for example a
  test that checks `c.verbose` after a `retries` change), instead of refusing
  with no ready repair. A test that already reads the field, a second
  candidate test, a binding passed elsewhere or shadowed, and a field the test
  cannot name still get no repair (#13700).
