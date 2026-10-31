<!-- section: Fixed -->
- `ripr check` no longer aborts with a stack overflow on a file with thousands of chained `let _ =`, `a = b = ..` or `for x in for y in ..` links; the nesting budget now counts right-associative `=` and `in` links and refuses the parse with `rust_nesting_budget` (#11410).
