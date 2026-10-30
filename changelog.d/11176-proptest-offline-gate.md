<!-- section: Fixed -->
- The two registered-property proptests now skip with a named reason on hosts
  whose Cargo cache lacks the fixture lockfile's crates, instead of failing
  with a misleading admitted-count error; warm hosts run the full assertions
  unchanged (#11176).
