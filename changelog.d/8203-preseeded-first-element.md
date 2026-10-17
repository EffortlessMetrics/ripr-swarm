<!-- section: Fixed -->
- A test that only reads the first element of a collection a changed
  `push` appends to no longer confirms that push when the element may
  predate it; the finding stays `weakly_exposed`, while a read of the
  pushed element still reads `exposed` (#8203).
