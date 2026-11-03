<!-- section: Fixed -->
- When a test passes its own empty vector or string as `&mut` to a function
  that pushes into it, and checks an exact count that only a pushed item can
  make true (`assert_eq!(b.len(), 1)`), `ripr check`
  now reads a deleted push as noticed. Both `ripr check` and `ripr repair` then
  start a repair on the changed pushed value. `ripr agent stub` writes that
  test as an exact check of the full contents, not a mock (#11402).
- `ripr check` no longer offers a repair start that `ripr repair` would
  refuse because there is no test file it may edit (#11402).
