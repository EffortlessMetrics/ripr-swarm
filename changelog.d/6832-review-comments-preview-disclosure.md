<!-- section: Fixed -->
- `review-comments` base/head runs now disclose that preview-language findings
  are not projected: the path is seam-scoped (Rust), so a Python or TypeScript
  diff yields `comments: 0` with run status `complete` even when diff-scoped
  `ripr check` has actionable evidence. When an enabled preview language has a
  changed file in the working set, the report carries a
  `preview_language_findings_not_projected` warning naming the changed files
  and the working route (`ripr reports gap-ledger --check-output ...` plus
  review-comments `--gap-ledger`), so an empty report is distinguishable from
  "no findings". `docs/LANGUAGE_ADAPTER_PREVIEW.md` no longer claims
  base/head review-comments works normally on all four languages (#6832).
