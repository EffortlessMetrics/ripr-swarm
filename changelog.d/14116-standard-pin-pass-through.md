<!-- section: Fixed -->
- A standard `pin!` (`std::pin::pin!` or `core::pin::pin!`, or a bare
  `pin!` that `use std::pin::pin;` names) no longer counts as a possible
  macro definition for the trusted names in its arguments. Before, a
  `format!` inside a pinned future blocked every `format!`-using assertion
  across the workspace (RIPR-SPEC-0369, #14116).
