<!-- section: Fixed -->
- Unique indexed inherent method names no longer grant direct test-call
  attribution without bounded receiver evidence. Diff and repository reports
  require the receiver to be established as the owner's type: a qualified
  annotation or factory path, a bare factory shadowed by an import, an enum
  variant imported only in a sibling module, and an inherent factory that is
  const-local, cfg-gated or generic over the owner's spelling no longer earn
  direct attribution. The broader refusal and preservation cases are pinned by
  follow-up hardening tests (#8004). Corpus snapshots
  retain the false actionable cost of unsupported factory flow (#7415, #8004).
