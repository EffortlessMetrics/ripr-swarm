<!-- section: Fixed -->
- The diff parser now enforces a language-neutral changed-line budget
  (`RIPR_MAX_DIFF_CHANGED_LINES`, default 100,000) while streaming, so one
  accepted file can no longer accumulate unbounded added/removed line payloads
  for any language. Over the limit it stops at the first line past the ceiling
  with the existing `diff_scope_oversized` failure; an invalid value fails
  closed. The stricter per-language limits are unchanged (#2398).
