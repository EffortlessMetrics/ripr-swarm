<!-- section: Fixed -->
- Rust: a function whose whole body is a constant changed to a different
  constant (`true` → `false`, `vec![1, 2]` → `vec![]`, cargo-mutants' FnValue
  mutant) now reads `exposed` under an exact assertion on its return and
  `reachable_unrevealed` under a bare call, instead of `propagation_unknown`.
  The base-side function body must itself be a whole-body constant, and the
  two constants must fold to different values; a named, computed or
  early-returned body on either side stays unknown (#14852).
