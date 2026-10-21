<!-- section: Fixed -->
- A check JSON findings array cut by the byte budget now also discloses it in
  `analysis_outcome` (`document_complete: false` with `findings_rendered` and
  `findings_total`, emitted only when bounded), and the shared incomplete-outcome
  predicate, the PR evidence summary (`run_status: incomplete`, producer
  `analysis_complete` preserved, counts and recovery rendered), and the xtask
  check-JSON readers fail closed on it, so a findings prefix never reads as a complete analysis. The
  typed analysis-outcome artifact validator (agent review summary) and the
  `review-comments --check-output` loader refuse both the flag and the legacy
  `run_limitations[]` shape, and the agent review-summary Markdown carries the
  refusal reason and recovery (#9010).
