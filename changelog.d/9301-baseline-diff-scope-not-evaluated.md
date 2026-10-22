<!-- section: Fixed -->
- `ripr baseline diff`, `ripr zero status` and `ripr baseline update
  --remove-resolved` no longer treat a baselined gap as `resolved` just
  because a diff-scoped gate decision does not list it. `gate evaluate` now
  discloses the candidate source's `analysis_scope` in `gate-decision.json`
  (additive field); a baseline entry absent from current evidence whose file
  was outside that scope, or whose decision discloses no scope, is reported as
  `stale_baseline_entry` with `evaluation: not_evaluated` and is kept by
  `--remove-resolved`. Gates built before this change fail closed until
  regenerated (#9301).
