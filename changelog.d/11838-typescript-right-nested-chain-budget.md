<!-- section: Fixed -->
- `ripr check` no longer aborts with a stack overflow on a TypeScript file with hundreds of thousands of right-nested `() => () => ..` arrows, function types or `a ? 1 : a ? 1 : ..` ternaries; the TypeScript nesting budget now counts these chains and refuses the parse with `expression_nesting_budget` (#11838).
