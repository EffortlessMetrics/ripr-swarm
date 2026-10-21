<!-- section: Fixed -->
- A check JSON findings array cut by the byte budget now also discloses it in
  `analysis_outcome` (`document_complete: false` with `findings_rendered` and
  `findings_total`, emitted only when bounded), and the shared incomplete-outcome
  predicate and the PR evidence summary fail closed on it, so a findings prefix
  never reads as a complete analysis (#9010).
