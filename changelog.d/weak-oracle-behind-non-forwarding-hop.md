<!-- section: Fixed -->
- A test that reaches a changed function only through a production caller
  that transforms its result, and whose every assertion on that caller is
  weak on its own terms (`assert!(banner().len() > 0)`,
  `assert!(!banner().is_empty())`), now reports the weak oracle's gap
  (`weakly_exposed`) instead of abstaining as `helper_result_not_forwarded`.
  Exact pins on the caller, plain `assert!(caller(..))`, and test-local
  helpers still abstain (#7549).
