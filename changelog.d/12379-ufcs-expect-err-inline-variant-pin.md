<!-- section: Fixed -->
- An inline `Result::expect_err(..)` or `std::result::Result::expect_err(..)`
  comparison against a named error variant is read as an exact-variant check
  like the method spelling, while error-replacing adapters (`map_err`,
  `or_else` and similar) in that position are not read as one (#12336, #12379).
