<!-- section: Internal / CI -->
- A seam-evidence pass that fails closed now hands its single pass-wide
  `producer_failure` limitation to the scoped and full seam-inventory results
  (and refuses to describe an unbuildable failure silently), with no output,
  schema or golden change; healthy walks are unchanged (#13340).
