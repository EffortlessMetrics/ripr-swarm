<!-- section: Fixed -->
- A test that calls a public wrapper of a changed helper now relates to the
  helper as `helper_owner_call` even when it also shares the helper's file,
  path or name tokens (unit tests beside a private helper, integration tests
  of a `pub` helper); it no longer reads as `same_test_file`,
  `owner_named_test` or `weak_token_substring` proximity (#6672).
- A predicate in a helper reached only through a wrapper now pairs with the
  wrapper's exact assertion at the boundary input when the wrapper returns
  the helper's result directly or branches on it into distinct literals
  (#6694).
- When tests reach a changed helper only through a wrapper that drops,
  binds, transforms or branches past its result, or that rebinds the
  forwarded parameter, propagation is unknown
  (`helper_result_not_forwarded`) instead of crediting the wrapper's
  assertion or reporting a gap; such a parameter also no longer carries the
  test's input into the helper's activation rows.
- The wrapper pairing above reads only boundary rows from the asserting
  test itself, so a same-line boundary input in another test file no longer
  pairs with an unrelated wrapper assertion.
- A scalar buried in a compound wrapper argument
  (`order_discount(std::cmp::max(10, 50))`, `order_discount(10 * 2)`) no
  longer pairs with the helper's boundary.
- A match guard that only reads the forwarded parameter (`n if n > qty =>`)
  no longer counts as rebinding it.
- A test that calls a forwarding intermediate caller of a changed helper is
  no longer stopped by an outer wrapper above it that drops the result; the
  forwarding check runs only up to the highest caller a related test calls
  directly.
- A test-local closure or nested fn named like a helper's wrapper
  (`let order_discount = |_: u32| 5;`) shadows it: calling it no longer
  relates the test to the helper as `helper_owner_call` or pairs the
  helper's boundary.
- A helper's side effect (`side_effect`, `call_deletion`) behind a wrapper
  that discards the helper's unit result keeps its propagation when every
  wrapper passes the effect's target through from its own parameter
  (`wrapper(out) { record(out) }`). An effect on a fresh temporary or a
  wrapper-local target abstains with `helper_result_not_forwarded`.
- A wrapper name that a test rebinds through a `use .. as` rename, a
  foreign-crate import, or a same-named `fn` in the test module no longer
  relates the test to the helper or pairs the helper's boundary.
