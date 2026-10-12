<!-- section: Fixed -->
- Oracle strength: an inequality no longer reads as an exact oracle
  (RIPR-SPEC-0231 rule 1). `assert_ne!` and `debug_assert_ne!` read
  `relational_check` / weak, or `whole_object_equality` / weak against a
  struct literal, and no probe family lifts that weak struct inequality back
  to the strong credit `assert_eq!` earns. So do `ensure!(a != b)` when `!=`
  is its only comparison, `assert!(!matches!(..))` (which previously read
  `exact_error_variant` / strong against `Err(E::X)`), and custom helpers
  with a `ne`, `not` or `neq` name segment such as `assert_not_equal`, even
  when an argument is `matches!(.., Err(E::X))`. A negated pattern counts as
  an inequality only when it is the whole condition: beside other `&&`
  conjuncts those conjuncts decide the reading, and inside `||` it caps the
  reading at weak. A seam whose only test was `assert_ne!(score(2), 0)`
  counted as strongly gripped in the repo audit; it is now weakly gripped
  (#6670).
