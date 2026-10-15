<!-- section: Fixed -->
- Unique indexed inherent method names no longer grant direct test-call
  attribution without bounded receiver evidence. Diff and repository reports
  refuse comment-only, expired-binding and assertion-operand item-shadow
  evidence while preserving actual enum receivers, public-library calls and
  outer receivers after an inner foreign shadow expires. Corpus snapshots
  retain the false actionable cost of unsupported factory/parameter/import flow
  (#7415, #8004).
- Same-name local wrapper delegation retains helper reach, while module-shadowed
  assertion operands and foreign helper receivers withhold high attribution
  consistently across diff and repository consumers (#7415, #8004).
