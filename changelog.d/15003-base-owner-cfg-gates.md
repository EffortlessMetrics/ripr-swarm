<!-- section: Fixed -->
- Rust: the shared base-owner identity behind the constant-tail and
  unary-tail return readings now reports every `cfg` gate it used to miss,
  so a gated owner keeps the old reading instead of a credited propagation
  witness: an inner `#![cfg(..)]`/`#![cfg_attr(..)]` in the function's own
  body (#15003); a `cfg` on a `use` that re-exports or renames to the owner,
  or globs its module; an owner file no `mod` declaration names (one reached
  only through `include!`); an `include!` naming the owner's file; and a
  `macro_rules!` that may emit a module (#14952). A `cfg(not(test))`
  re-export of the owner beside a test-only stand-in now reads
  `propagation_unknown` instead of `weakly_exposed`; ungated and unrelated
  re-exports keep their credit. These gates are kept narrow: a gated `use`
  rooted at an unshadowed `std`/`core`/`alloc` path, a private
  `#[cfg(..)] use super::name;` that is the only binding of `name` in a
  `#[cfg(test)]` module, an `include!` resolving to another file of the same
  stem, a macro emitting only inline modules, and a non-exported
  module-emitting macro of another package keep their credit.
