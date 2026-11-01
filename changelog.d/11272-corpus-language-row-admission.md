<!-- section: Fixed -->
- The verdict-corpus harness now also requires the producer's per-language
  `language_completeness` row for the expected language to be evidenced
  (`complete`, `complete_zero` or `partial`) before scoring a live case, so an
  aggregate that reads complete because a sibling language ran no longer
  admits a case whose own adapter was skipped, unavailable or failed. #11272.
