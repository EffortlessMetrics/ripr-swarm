<!-- section: Added -->
- `cargo xtask work context check` validates committed context/work-item
  packet rows against the compiled work-portfolio basis: every packet binds
  the repository, portfolio-snapshot identity, basis head and the
  selected-work identity it compiles context for, reusing the
  RIPR-SPEC-0244 selection law engine; the delegation scope is a closed
  bounded vocabulary and every carried context/overflow reference must stay
  relevant to the bound selection (campaign references are context, never
  permission), a standalone issue with no campaign placement still
  compiles a valid packet, and an unresolvable legacy ref, an irrelevant
  reference, unbounded overflow, an evidence-free packet, a wrong
  repository or a stale basis fails with exact recompile/reconcile routes
  (#1706).
