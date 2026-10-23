<!-- section: Fixed -->
- A same-file test helper that receives the owner call as a closure
  (`run_test("hi bob", || greet("bob"))`, where the helper invokes the
  closure parameter and then asserts) now lends its assertion to the calling
  test, so the test is no longer reported as "has no assertion". A helper
  that never invokes the parameter, asserts only before invoking it, rebinds
  it, or invokes it only inside a closure still grants nothing (#9981).
