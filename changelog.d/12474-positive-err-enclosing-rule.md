<!-- section: Fixed -->
- An `Err(..)` assertion is credited only when every enclosing call up to the
  assertion macro is on the positive allowlist (`Some`, `Ok`, `eq`, and the
  assertion macros themselves). Wrapper calls such as `id(Err(..))`, receiver
  reads such as `Err(V).unwrap_err()`, and `Some(Err(V)).unwrap()` chains no
  longer count as exact error-variant pins (#12474).
