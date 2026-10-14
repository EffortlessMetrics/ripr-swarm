<!-- section: Fixed -->
- A replaced line whose names are all shorter than three characters, such as
  `if x > 0 {` to `if x >= 0 {`, is probed once as the replacement instead of
  being reported again as deleted behavior. A renamed short binding, a
  changed constructor, a different probe family, two short-name deletions
  next to one added predicate, comment-only names, and a genuinely unpaired
  deletion still keep the removed probe (#7807).
