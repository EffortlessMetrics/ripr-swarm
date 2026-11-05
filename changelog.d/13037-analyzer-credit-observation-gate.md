<!-- section: Fixed -->
- Rust analysis: an exact pin now credits a changed scalar early return
  (`return 100;`) and a changed scalar tail beside early returns when every
  sibling exit builds a different whole scalar literal, so the changed exit is
  the owner's only source of its value and a passing exact assertion proves it
  ran. The gate stays fail-closed: a second exit producing the same literal, a
  parameter or computed exit, a `?`, a nested-closure return, an unbounded
  macro, or a conditionally-evaluated changed value receives no credit, and a
  pin on a sibling exit's value alone still does not confirm the probe
  (#13037, Defect A).
