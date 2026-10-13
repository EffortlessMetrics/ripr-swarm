<!-- section: Fixed -->
- An `ensure!` condition is exact only when a top-level `&&` conjunct is
  itself an `==` comparison outside every `||` arm.
  `ensure!(a != b || c == d)`, `ensure!(f(|x| x == 1))` and
  `ensure!(xs.iter().position(|x| *x == 1) != None)` used to read
  `exact_value` / strong and now read `relational_check` / weak
  (RIPR-SPEC-0231 rule 1). In `assert!`, `debug_assert!`, `prop_assert!`
  and `ensure!`, a disjunction of `matches!` patterns or error variants
  (`assert!(matches!(r, Ok(3)) || fallback)`) no longer reads
  `exact_value` or `exact_error_variant`, and `ready || r.is_err()` reads
  `relational_check` rather than `broad_error`, as
  `ready || events_sent.contains(&id)` does rather than
  `mock_expectation`. A parenthesized `||`
  conjunct drops out and the other conjuncts decide, so
  `ensure!(c == d && (a || b))` stays `exact_value` (#7558).
