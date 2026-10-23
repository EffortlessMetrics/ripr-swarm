<!-- section: Fixed -->
- A same-file test helper that receives the owner call as a closure
  (`run_test("hi bob", || greet("bob"))`, where the helper invokes the
  closure parameter and then asserts) now lends its assertion to the calling
  test, so this directly invoked form is no longer reported as "has no
  assertion". The new callback credit requires an eager, immutable callback
  invocation followed by a unique parser-backed assertion. Forwarded and
  unused closure parameters remain outside this repair (#9981).
