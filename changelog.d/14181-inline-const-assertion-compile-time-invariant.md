<!-- section: Changed -->
- Rust checks: the finding on an inline const assertion inside a function
  (`const { assert!(N <= 249) };`) now names the `compile_time_invariant`
  stop reason, like item-level `const_assert!` lines. A violating edit fails
  to compile, so no test observes it (#14181).
