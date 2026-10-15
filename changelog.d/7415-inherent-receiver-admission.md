<!-- section: Fixed -->
- Unique indexed inherent method names no longer grant direct test-call
  attribution without bounded receiver evidence. Diff and repository reports
  refuse comment-only, expired-binding and assertion-operand item-shadow
  evidence while preserving actual enum receivers and public-library calls
  (#7415, #8004).
