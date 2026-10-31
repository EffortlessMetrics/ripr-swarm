<!-- section: Fixed -->
- `ripr check` no longer aborts with a stack overflow on a file with thousands of consecutive `return`, `break`, `yield`, `become`, `if`, `match` or `while` keywords; the nesting budget now counts them like prefix operators and refuses the parse with `rust_nesting_budget` (#11407).
