<!-- section: Fixed -->
- Rust: a changed constant tail that is the function's whole body and carries
  no call, digit, string or spaced arithmetic (`false`, `vec![]`,
  `[b'a', b'b']`) now reaches the return instead of reading
  `propagation_unknown`, so an exact assertion reads `exposed` and a bare call
  `reachable_unrevealed`. A tail that names a parameter, local, constant or
  field (`-a`, `a & b`, `p.x`), and comparison or `&&`/`||` tails, stay as
  before (#14797, #14815).
