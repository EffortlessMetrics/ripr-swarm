<!-- section: Fixed -->
- `ripr baseline diff`, `ripr zero status` and `ripr baseline update
  --remove-resolved` no longer treat a baselined gap as `resolved` just
  because a diff-scoped gate decision does not list it. `gate evaluate` now
  discloses the candidate source's `analysis_scope` in `gate-decision.json`
  (additive field); a baseline entry absent from current evidence whose file
  was outside that scope, or whose decision discloses no scope, is reported as
  `stale_baseline_entry` with `evaluation: not_evaluated` and is kept by
  `--remove-resolved`. Gate decisions written before this change carry no `analysis_scope`, so
  every absent baseline entry reads `not_evaluated` until the gate decision is
  regenerated. Seams dropped by the brief cap, skipped seams, and malformed
  scope disclosures also fail closed. `--gap-ledger` gate decisions always
  read `not_evaluated` until the ledger path discloses analyzed files (#9599)
  (#9301).
