<!-- section: Fixed -->
- A standard `pin!` (`std::pin::pin!` or `core::pin::pin!`, or a bare
  `pin!` that `use std::pin::pin;` names) no longer counts as a possible
  macro definition for the trusted names in its arguments. Before, a
  `format!` inside a pinned future blocked every `format!`-using assertion
  across the workspace (RIPR-SPEC-0369, #14116).
- A local `macro_rules!` named after an audited pass-through wrapper (`pin`
  or `proptest`) now counts as a possible redefinition only when it pastes a
  fragment that can carry an item or statement. A counterfeit `pin!` that
  takes `$f:expr` no longer blocks assertion credit in its crate (#14116).
