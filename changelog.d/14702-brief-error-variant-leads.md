<!-- section: Fixed -->
- `ripr agent brief` lists a changed `return Err(..)` line's `error_variant`
  seam, which carries the concrete `expect_err` + `matches!` assertion, ahead
  of the `return_value` seam on the same line. The seam-id hash used to
  decide, so an unresolved seam could lead the brief and push a concrete one
  past its cap (#14702).
