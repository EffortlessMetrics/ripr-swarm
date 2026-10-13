<!-- section: Fixed -->
- Completes the RIPR-SPEC-0231 rule 1 landing (#6670) in the verdict
  corpus: the five rust oracle rows for inequality oracles
  (`assert_ne` ok-value/scalar/struct-literal, `ensure!(!=)`, and a
  ne-named helper) now record the accepted `relational_check` /
  `whole_object_equality` with `weak` strength instead of the retired
  `exact_value` / `strong`, and the public-proof verdict-corpus receipt
  is refreshed to match the blessed rows (#6670).
