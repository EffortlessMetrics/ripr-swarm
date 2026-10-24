<!-- section: Fixed -->
- A same-file test helper that receives the owner call as a closure literal
  (`run_test("hi bob", || greet("bob"))`) and eagerly invokes that parameter
  as a straight-line statement (`let got = f(); assert_eq!(got, want);`) now
  lends the assertions that follow the invocation to the calling test, so the
  test is no longer reported as "has no assertion". Invocations under
  `if`/`match`/loops, after an early exit or an unrecognized macro, in a
  nested closure or macro
  argument, a `return`, `?` or `yield` hidden in a macro operand or written
  inside the callback closure itself, helpers that invoke two or more closure parameters, boxed or
  by-reference closures and forwarded callbacks (the
  tracing `with_default(subscriber, fun)` shape) are not credited by this
  change (#9981).
