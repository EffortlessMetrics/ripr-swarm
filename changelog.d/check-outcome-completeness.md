<!-- section: Fixed -->
- The check JSON schema rejects completeness, limitation, and outcome-count
  combinations forbidden by the existing typed analysis-outcome contract,
  including zero/nonzero disagreements with summary totals. Full positive-total
  equality remains a semantic consumer check, covered by the existing DX
  scoreboard admission predicate (#2829).
