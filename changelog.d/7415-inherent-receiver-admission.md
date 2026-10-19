<!-- section: Fixed -->
- Unique indexed inherent method names no longer grant direct test-call
  attribution without bounded receiver evidence. Diff and repository reports
  refuse comment-only, expired-binding and assertion-operand item-shadow
  evidence while preserving actual enum receivers, public-library calls and
  outer receivers after an inner foreign shadow expires. Accepted cost: three
  verdict-corpus rows (`lib-rstest-fixture-injected-receiver`,
  `serde-derive-rename-field-upper`, `serde-derive-rename-variant-lower`) move
  from credited/ideal to gap/`false_actionable`. Their tests do discriminate;
  the bounded rule cannot yet prove the rstest-injected receiver or the
  bare-imported unit variant (`UpperCase.apply_to_field`), so the shape is
  reported as a gap, a false actionable signal on a discriminating test, in
  exchange for refusing unproven receiver credit elsewhere. Restoring credit for those shapes is tracked in #8782
  (#7415, #8004).
- Same-name local wrapper delegation retains repository helper reach; diff
  evidence remains conservative. Module-shadowed assertion operands and foreign
  inherent helper receivers withhold high attribution (#7415, #8004).
- Repository direct calls share the complete owner/module refusal with diff
  calls. Dormant nested functions, closures and async bodies do not supply
  executed receiver calls; free-helper entries and intermediate hops bind to
  their actual module/function identity instead of a shared name. Function type
  parameters cannot borrow same-spelled module or owner-type credit (#7415, #8004).
- Standard assertion operands also exclude unpolled async bodies from direct
  and free-helper owner credit, while preserving genuine outer calls (#8004).
- Unpolled async-function helpers no longer lend inherent-owner execution
  credit at entry, assertion operand, intermediate or last-hop traversal.
  Genuine synchronous and mixed helper routes retain their bounded credit;
  polling inference remains unsupported (#7415, #8004).
- Complete-source declaration and lexical facts are reused within each source
  snapshot across helper candidates and owners, retaining ambiguous/unknown
  declaration refusal and conservative scope boundaries (#7415, #8004).
- Test-module shadows share exact-source parse, line and ancestry facts and
  complete test/owner/type-or-trait answers. Executable callers and assertion
  operands reuse immutable syntax while retaining original lexical anchors and
  conservative execution and binding refusals (#7415, #8004).
