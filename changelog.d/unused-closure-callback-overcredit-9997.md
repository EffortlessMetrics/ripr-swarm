<!-- section: Fixed -->
- A same-file test helper that never invokes a closure parameter, or only
  forwards it (the tracing `with_default(subscriber, fun)` shape), no longer
  lends its assertions to a test that passes it a call-making closure literal,
  including `&|| ..` and `Box::new(|| ..)`
  (`run_test("hi bob", || greet("bob"))`), because those assertions cannot be
  shown to observe what the closure called. Helpers that eagerly invoke the
  parameter keep their credit (#9997).
  A closure bound to a name first or a function item is not recognized and
  keeps the old credit, and the withholding is per helper, so a helper whose
  own assertion observes the owner also loses credit for such a call.
  Closures an iterator or `Option` adapter runs inside the argument
  (`.map(|v| gate(*v))`, `.any(..)`, `.unwrap_or_else(..)`) are withheld too, a
  deliberate fail-closed missed advisory.
