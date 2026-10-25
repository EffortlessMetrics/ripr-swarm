<!-- section: Fixed -->
- Unique indexed inherent method names no longer grant direct test-call
  attribution without bounded receiver evidence. Diff and repository reports
  refuse comment-only, expired-binding and assertion-operand item-shadow
  evidence, calls inside dormant nested functions, closures and async bodies, and
  foreign inherent helper receivers, while preserving actual enum receivers,
  declared nominal parameter types and public-library calls. Corpus snapshots
  retain the false actionable cost of unsupported factory flow (#7415, #8004).
