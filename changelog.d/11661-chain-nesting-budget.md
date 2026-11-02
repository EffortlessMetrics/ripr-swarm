<!-- section: Fixed -->
- `ripr check` no longer aborts with a stack overflow on files with tens of thousands of chained `::` path segments, `as` casts, call or index suffixes, or `box` patterns; the nesting budget counts them and refuses the parse with `rust_nesting_budget` (#11661).
