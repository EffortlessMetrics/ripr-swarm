<!-- section: Fixed -->
- Rust: an integer whole-body constant tail (`fn ready() -> i32 { 1 }`
  changed to `{ 2 }`) no longer reads `weakly_exposed` or `exposed` with
  propagation credited when a `cfg` gate may swap the owner for a twin the
  tests run instead: a `cfg`/`cfg_attr` on the function, an enclosing
  module or the file, a same-path `cfg` twin, or a gated module declaration
  on its chain. Such a tail now reads `propagation_unknown`, the same
  shared base-owner gate decision the bool and unary tail readings use
  (#15104). Ungated owners, including one beside an unrelated `cfg(test)`
  module or a same-named function on another path, keep their credit.
  The gate fails closed: a workspace file whose `mod` declarations cannot
  be parsed refuses the credit too. Other returned shapes (`format!`,
  `concat!`, `Ok(..)` and early-return tails) are not covered yet (#15198).
- Rust: a `#[path]` module declaration (such as a
  `#[cfg(test)] #[path = "test_support.rs"] mod tests;`) no longer gates
  owners in a default crate root (`src/lib.rs`, `src/main.rs`) whose file
  name no `#[path]` literal names; other owners still read it as able to
  retarget them (#15104).
