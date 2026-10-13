<!-- section: Fixed -->
- A test that reaches a changed function returning a std string or scalar
  only through a production caller that carries its result on as data
  (`format!("[{}]", title())`, `title().to_string() + "!"`), and whose
  every assertion on that caller is a plain `assert!` with an ordering or
  `!=` comparison or an `is_empty`/`is_some`/`is_none`/`is_ok`/`is_err`
  check, now reports the weak oracle's gap (`weakly_exposed`) instead of
  abstaining as `helper_result_not_forwarded`. A caller that branches on
  the result or passes it to any other call, a user-defined return type,
  an exact or `==` pin, a bare `assert!(caller(..))`,
  `?`/`unwrap`/`expect` in the test and test-local helpers still abstain.
  Across a local-macro argument hop the weak-oracle exception is now this
  same narrower assertion rule, so a `.contains(..)` or bare-call check
  there abstains too (#7549).
