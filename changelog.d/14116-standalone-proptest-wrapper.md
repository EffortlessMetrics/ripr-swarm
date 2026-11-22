<!-- section: Fixed -->
- A `prop_oneof!` (or another proptest wrapper) used on its own, such as in a
  strategy helper outside `proptest!`, no longer counts as a possible macro
  definition for the trusted names in its arguments. Before, a `vec!` in its
  arguments blocked every `vec!`-using assertion across the workspace
  (RIPR-SPEC-0369, #14116).
