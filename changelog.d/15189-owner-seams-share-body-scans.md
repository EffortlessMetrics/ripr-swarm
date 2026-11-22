<!-- section: Fixed -->
- Rust analysis masks an owner body, resolves a changed binding's uses, and
  reads a predicate line once for all of the owner's boundary seams instead
  of once per seam, and never searches the body for a numeric literal as if
  it were a local, so a long `else if` chain is checked several times faster
  (#15189).
