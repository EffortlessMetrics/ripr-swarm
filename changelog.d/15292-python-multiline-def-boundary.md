<!-- section: Fixed -->
- A Python predicate boundary flip in an owner whose `def` spans several lines no
  longer reads as `exposed` when no test places the boundary: the signature's
  `max_age: int | None = None,` continuation lines were read as body rebindings,
  which left the boundary activation unknown and kept the oracle verdict
  (#15292).
