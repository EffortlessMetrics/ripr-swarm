<!-- section: Fixed -->
- TypeScript `toBe(-1)` and `toBe(+1)` now read as literal expected values.
  A unary `-` or `+` applied directly to a numeric literal is no longer
  reported as `typescript_dynamic_assertion_unresolved`. `-x`, `-f()`, and
  `!true` stay dynamic. A dead signed expectation on a predicate boundary
  still does not witness (#7647).
