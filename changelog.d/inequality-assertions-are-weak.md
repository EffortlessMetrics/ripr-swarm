<!-- section: Fixed -->
- Oracle strength: an inequality no longer reads as an exact oracle
  (RIPR-SPEC-0231 rule 1). `assert_ne!` and `debug_assert_ne!` read
  `relational_check` / weak, or `whole_object_equality` / weak against a
  struct literal; so do `ensure!(a != b)` when `!=` is its only comparison,
  `assert!(!matches!(..))` (which previously read `exact_error_variant` / strong
  against `Err(E::X)`), and custom helpers with a `ne`, `not` or `neq` name
  segment such as `assert_not_equal`. A seam whose only test was
  `assert_ne!(score(2), 0)` counted as strongly gripped in the repo audit;
  it is now weakly gripped (#6670).
