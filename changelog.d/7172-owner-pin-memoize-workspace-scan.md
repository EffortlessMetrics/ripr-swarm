<!-- section: Changed -->
- The unit-struct receiver pin (#7083) no longer rescans the workspace for
  every assertion it admits. The name-independent scans (the module-name
  set, the glob-reaches-outside, `include!`/`#[path]` and `:ident`/`:tt`
  macro checks) now run once per classification run over the shared
  run-scoped syntax state, and only the per-name spelling checks run per
  admission. Verdicts are unchanged; on a synthetic 301-file index with 40
  unit-struct admissions, the admissions drop from ~1.34s to ~0.25s with
  no measurable RSS change (#7172).
