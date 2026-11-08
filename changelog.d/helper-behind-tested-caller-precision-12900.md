<!-- section: Fixed -->
- A private helper whose only related tests share its file or a name token no
  longer reads `reachable_unrevealed` when a chain of production callers leads
  to a function the tests do call (a helper behind a public entry). Those
  nearby tests are not shown to run the helper, so their missing assertion is
  no evidence the change goes unnoticed; the finding reads `weakly_exposed`
  instead. On the five-crate cargo-mutants corpus this removes 61 false
  positive gaps; 8 boundary misses on helpers the tests do run move to
  `weakly_exposed` with them (#12900).
