<!-- section: Fixed -->
- An `assert_eq!` inside a test-local check helper now counts for the test
  that calls it on an ordinary statement path, so a table of pins such as
  `check_tip(40, 6, 46); check_tip(10, 0, 10);` over
  `fn check_tip(..) { assert_eq!(with_tip(b, t), want); }` reads `exposed`
  instead of `weakly_exposed` with "assertion not credited". A boundary
  input passed through the helper's call (`check_pass(50, true)` over
  `assert_eq!(passes(score), want)`) now pairs with that assertion, so the
  changed predicate reads `exposed` too. The helper must be a plain,
  non-generic function defined once in the test's own module, with no early
  exit, and the call must not sit in a loop, branch or deferred closure;
  anything else stays refused (RIPR-SPEC-0197 rule 7, RIPR-SPEC-0186,
  [#6482](https://github.com/EffortlessMetrics/ripr-swarm/issues/6482)).
