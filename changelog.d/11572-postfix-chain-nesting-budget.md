<!-- section: Fixed -->
- `ripr check` no longer aborts with a stack overflow on thousands of chained `as` casts, `()` calls, `[0]` indexes, `x @ y @` bindings or `box` patterns; the nesting budget now counts them and refuses the parse with `rust_nesting_budget` (#11572).
