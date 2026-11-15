<!-- section: Fixed -->
- The human `Related test` headline, the JSON finding `oracle_kind` /
  `oracle_strength` summary, the agent `open=` test row and the test a `done=`
  line names as noticing the change now follow the same
  observing-first rule as the agent `tK=` row and the context fix site: a test
  whose assertion cannot observe the change, or that has no call path to it,
  is not named while one that can is listed. The JSON summary therefore no
  longer reports a strong oracle that only an unreached test holds; it can now
  read `weak` or `none` where the tests that reach the change assert weakly or
  not at all. On a changed `Err` variant, every surface names the
  `assert!(f(x).is_err())` test to tighten instead of the strong `Ok` pin
  (#14483).
