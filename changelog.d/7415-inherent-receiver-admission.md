<!-- section: Fixed -->
- Unique indexed inherent method names no longer grant direct test-call
  attribution without bounded receiver evidence. Diff and repository reports
  refuse comment-only, expired-binding and assertion-operand item-shadow
  evidence while preserving actual enum receivers, public-library calls and
  outer receivers after an inner foreign shadow expires. Corpus snapshots
  retain the false actionable cost of unsupported factory flow (#7415, #8004).
- Same-name local wrapper delegation retains repository helper reach; diff
  evidence remains conservative. Module-shadowed assertion operands and foreign
  inherent helper receivers withhold high attribution (#7415, #8004).
- Repository direct calls share the complete owner/module refusal with diff
  calls. Dormant nested functions, closures, async bodies and unpolled
  async-function helpers do not supply executed owner credit (entry, assertion
  operand, intermediate or last hop); free-helper entries and intermediate hops
  bind to their actual module/function identity, and function type parameters
  cannot borrow same-spelled module or owner-type credit (#7415, #8004).
- Complete-source declaration, lexical, parse and shadow facts are reused within
  each source snapshot across helper candidates, owners, executable callers and
  assertion operands, retaining ambiguous/unknown declaration refusal and
  conservative scope boundaries (#7415, #8004).
- The inherent-receiver check reads a declared nominal parameter type
  (`fn t(account: Account)`, including an `#[rstest]` fixture injection) and a
  bare enum variant brought in by a single `use Enum::*` as visible receiver
  bindings; aliases, prelude spellings, shadowing items and ambiguous imports
  still withhold (#9676).
