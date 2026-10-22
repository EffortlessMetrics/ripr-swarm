<!-- section: Fixed -->
- Candidate admission's size envelope counts only reviewable lines: files declared
  `linguist-generated` in the base commit's `.gitattributes` (lockfiles, blessed
  `expected/` rows, public-proof metrics) are excluded, limited to data file
  types and a separate generated-line cap, so a lockfile or corpus regeneration
  no longer pushes a PR over the ordinary limit (#9261).
