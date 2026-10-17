<!-- section: Fixed -->
- A test that pins one element of a returned array by index no longer earns
  `exposed` for that element's field when a sibling element builds the same
  field from a block, call, or other expression that may exit the process
  before the assertion runs; the finding stays `weakly_exposed` (#8084).
