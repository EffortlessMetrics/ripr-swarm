<!-- section: Fixed -->
- A pure bool `&&`/`||` owner is no longer credited from truth-table rows when the workspace rebinds `assert`, `assert_eq` or `assert_ne` (for example a `macro_rules! assert_eq` that discards its operands). The reading trusted the macro's name to compare; it now declines and keeps `infection_unknown` (#10386).
