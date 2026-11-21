<!-- section: Fixed -->
- Rust assertions now read the expected binding in scope at the comparison.
  A same-named closure parameter or match arm elsewhere in the test no longer
  hides an unfilled placeholder, and assertions inside those scopes keep
  credit for the real value (#14868).
