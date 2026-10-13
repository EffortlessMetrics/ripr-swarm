<!-- section: Fixed -->
- An `ensure!` condition is exact only when a top-level `==` sits outside
  every `||` arm. `ensure!(a != b || c == d)`, `ensure!(f(|x| x == 1))` and
  `ensure!(xs.iter().position(|x| *x == 1) != None)` used to read
  `exact_value` / strong and now read `relational_check` / weak
  (RIPR-SPEC-0231 rule 1). A disjunction of error variants no longer reads
  `exact_error_variant` either. `assert!` already read these forms weak
  (#7558).
