<!-- section: Fixed -->
- Rust: a constant tail that is the function's whole body, replaced by
  another constant that carries no call, digit, string or spaced arithmetic
  (`true` to `false`, `vec![1, 2]` to `vec![]`), now reaches the return
  instead of reading `propagation_unknown`. An exact assertion reads `exposed`
  and a bare call reads `reachable_unrevealed`. These tails stay as before:
  a tail that names a parameter, local, constant or field (`-a`, `a & b`,
  `p.x`), a constant replacing such a tail (`flag` to `false`), and
  comparison or `&&`/`||` tails (#14797, #14815).
