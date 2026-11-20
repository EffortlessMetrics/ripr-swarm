<!-- section: Fixed -->
- Rust analysis decides once per owner body whether a comparison operand is a
  local alias of a parameter, instead of re-reading the body for every
  boundary seam, so an owner with a long `else if` chain no longer slows down
  quadratically (#15189).
