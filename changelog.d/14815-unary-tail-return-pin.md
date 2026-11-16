<!-- section: Fixed -->
- Rust: deleting or adding a unary `-`/`!` on a returned parameter (`-a` →
  `a`, cargo-mutants' UnaryOperator) no longer reads `propagation_unknown`.
  When the diff changes only that one-line whole-body tail, the change sinks
  to the owner's return: an exact pin where the versions differ reads
  `exposed` and a bare call `reachable_unrevealed`. Because `-a` and `a`
  agree at `0`, a pin at `0` reads `weakly_exposed`, and an argument ripr
  cannot fold to a literal (a local, a loop row, a constant) stays
  `infection_unknown` (#14815).
