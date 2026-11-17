<!-- section: Fixed -->
- Rust: the shared base-owner identity behind the unary-tail return reading
  now reports every `cfg` gate it used to miss, so a gated owner keeps the old
  reading instead of a credited propagation witness: an inner
  `#![cfg(..)]`/`#![cfg_attr(..)]` in the function's own body (#15003); a
  `cfg` on a `use` that re-exports or renames to the owner, or globs its
  module; an owner file no `mod` declaration names (one reached only through
  `include!`); an `include!` naming the owner's file; and a `macro_rules!`
  that may emit a module (#14952). A `cfg(not(test))` re-export of the owner
  beside a test-only stand-in now reads `propagation_unknown` instead of
  `weakly_exposed`; ungated and unrelated re-exports keep their credit.
