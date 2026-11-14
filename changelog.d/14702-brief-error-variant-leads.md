<!-- section: Fixed -->
- `ripr agent brief` folds the `return_value` seam on a changed
  `return Err(..)` line into the `error_variant` seam for the same
  expression, which carries the concrete `expect_err` + `matches!` assertion.
  The twin no longer spends a brief slot or pushes a concrete seam past the
  cap, and a warning states how many seams were folded. On the agent-eval
  `error_variant` scenario the brief drops from 2,578 to 2,013 est. tokens
  (#14702).
