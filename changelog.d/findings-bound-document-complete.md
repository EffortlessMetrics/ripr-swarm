<!-- section: Fixed -->
- A check JSON findings array cut by the byte budget now also discloses it in
  `analysis_outcome` (`document_complete: false` with `findings_rendered` and
  `findings_total`, emitted only when bounded), and the shared incomplete-outcome
  predicate, the PR evidence summary, and the xtask check-JSON readers fail
  closed on it, so a findings prefix never reads as a complete analysis. The
  artifact-projection readers (`agent_review_summary`, packet currentness) are
  out of scope (#9010).
