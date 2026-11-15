<!-- section: Fixed -->
- The human `Related test` headline and the JSON `oracle_kind` /
  `oracle_strength` summary now follow the same observing-first rule as the
  agent `tK=` row and the context fix site: a test whose assertion cannot
  observe the change (or that has no call path to it) is not named while one
  that can is listed. On a changed `Err` variant, every surface now names the
  `assert!(f(x).is_err())` test to tighten instead of the strong `Ok` pin
  (#14483).
