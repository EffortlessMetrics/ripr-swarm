<!-- section: Fixed -->
- A Rust test that checks a bool function with `assert!(is_over(x))` or
  `assert!(!is_over(x))` now counts as pinning that function's whole result
  (RIPR-SPEC-0197). Before, ripr read it as a weak relational check and
  reported a boundary the test checks on both sides as `weakly_exposed`.
  On the verdict corpus, false actionable verdicts drop from 33/56 to 30/56,
  with no case moving toward a false verdict.
