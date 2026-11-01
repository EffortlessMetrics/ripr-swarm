<!-- section: Fixed -->
- `assert_eq!(f(..).unwrap_err(), MyError::Variant)` now counts as an exact
  error-variant pin, as `let e = f(..).unwrap_err(); assert_eq!(e, MyError::Variant)`
  already did. Before, a direct test written that way never discriminated an
  `error_variant` seam, so the seam was strongly gripped only when an unrelated
  test lent it credit (#12266).
