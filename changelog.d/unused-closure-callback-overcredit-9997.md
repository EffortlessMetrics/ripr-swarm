<!-- section: Fixed -->
- A same-file test helper that never invokes a closure parameter, or only
  forwards it (the tracing `with_default(subscriber, fun)` shape), no longer
  lends its assertions to a test that passes a call-making closure literal
  (`run_test("hi bob", || greet("bob"))`), because those assertions cannot be
  shown to observe what the closure called. Helpers that eagerly invoke the
  parameter keep their credit (#9997).
