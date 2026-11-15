<!-- section: Fixed -->
- Rust: a changed tail value that is the function's whole body and carries no
  call, digit, string or spaced arithmetic (`false`, `vec![]`, `-delta`,
  `mask & bits`) now reaches the return instead of reading
  `propagation_unknown`, so an exact assertion at an input where old and new
  differ reads `exposed`, one where they agree `weakly_exposed`, and a bare call
  `reachable_unrevealed`. Tails ripr cannot evaluate at the test's inputs
  (`-a` to `a`, field reads, casts, a shadowed operand) and comparison or
  `&&`/`||` tails stay as before (#14797, #14815).
