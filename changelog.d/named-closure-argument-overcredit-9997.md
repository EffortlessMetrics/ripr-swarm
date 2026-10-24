<!-- section: Fixed -->
- A same-file test helper that never invokes a closure parameter no longer
  lends its assertions to a test that passes it a closure bound to a name first
  (`let c = || greet("bob"); run_test("hi bob", c);`, also `&c` and `(c)`), the
  same withholding a closure literal already got (#9997). Rebinding or `mut` on
  the name does not exempt it (a deliberate missed advisory). A function item
  passed by name and a boxed named closure remain a documented residual and keep
  the old credit.
