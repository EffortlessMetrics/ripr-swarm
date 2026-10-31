<!-- section: Fixed -->
- `ripr check` no longer aborts with a stack overflow on a TypeScript file with hundreds of thousands of chained `a < a < ..` comparisons or nested `Array<Array<..>>` type arguments; the TypeScript nesting budget now counts open `<` runs and refuses the parse with `expression_nesting_budget` (#11661).
