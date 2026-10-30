<!-- section: Fixed -->
- A test that binds `let q = assert_ok!(owner(..))` (or `assert_some!`) from a
  manifest-verified `use claims::..` import and compares `q` with a literal
  now pins an `Ok`/`Some` tail like `owner(..).unwrap()`; foreign, renamed,
  redefined or unverified `assert_ok!` stays opaque (#9619).
