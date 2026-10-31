<!-- section: Fixed -->
- `ripr check` no longer aborts with a stack overflow on files with tens of thousands of left-nested `x @ y @ ..` binding links or `break 'a break 'a ..` label chains; the nesting budget counts them and refuses the parse with `rust_nesting_budget` (#11572).
