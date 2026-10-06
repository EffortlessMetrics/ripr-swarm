<!-- section: Fixed -->
- TypeScript rows with no recognized assertion now say whether ripr
  established that the test has no assertion
  (`extraction_complete_no_assertion`) or saw something that could still fail
  it (`assertion_like_present_but_unresolved`): a custom matcher, a same-file
  or test-support helper, a third-party or unknown callee, `throw`, a
  rejected promise, `done(err)`, a test-context member such as `t.plan`, or an
  asserting hook or setup code in the file. Only the established state can
  later justify a "no assertion" reason. Verdicts are unchanged (#5524).
