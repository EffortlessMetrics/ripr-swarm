<!-- section: Fixed -->
- Unique indexed inherent method names no longer grant direct test-call
  attribution without bounded receiver evidence. Diff and repository reports
  refuse comment-only, expired-binding and assertion-operand item-shadow
  evidence while preserving actual enum receivers, public-library calls and
  outer receivers after an inner foreign shadow expires. Corpus snapshots
  retain the false actionable cost of unsupported factory/parameter/import flow
  (#7415, #8004).
- Same-name local wrapper delegation retains repository helper reach; diff
  evidence remains conservative. Module-shadowed assertion operands and foreign
  inherent helper receivers withhold high attribution (#7415, #8004).
- Repository direct calls share the complete owner/module refusal with diff
  calls. Dormant nested functions, closures and async bodies do not supply
  executed receiver calls; free-helper entries and intermediate hops bind to
  their actual module/function identity instead of a shared name. Function type
  parameters cannot borrow same-spelled module or owner-type credit (#7415, #8004).
