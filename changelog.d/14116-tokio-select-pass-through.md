<!-- section: Fixed -->
- A `tokio::select!` no longer counts as a possible macro definition for
  the trusted names in its branches when the locked tokio is the audited
  1.54.1. Before, a `format!` in a branch handler blocked every
  `format!`-using assertion across the workspace (RIPR-SPEC-0369, #14116).
