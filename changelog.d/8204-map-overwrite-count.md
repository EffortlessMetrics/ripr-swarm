<!-- section: Fixed -->
- A `len()` assertion no longer confirms deleting a map `insert`
  when the test already holds the inserted key, since overwriting
  it leaves the count unchanged; the finding stays
  `weakly_exposed`, while an empty-start insert still reads
  `exposed` (#8204).