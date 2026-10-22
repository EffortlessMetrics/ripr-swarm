<!-- section: Fixed -->
- TypeScript: a callback passed to a call in a mocha `done` test and settled by
  `done()` reads as the test's own assertions, and a boundary expectation that
  still pins the unchanged value keeps the boundary witness, so
  `mocha-done-callback-value` and `mocha-nested-describe-boundary` move from
  false actionable to `exposed`. A registration inside a skipped `describe` is
  no longer disclosed as a dropped one.
