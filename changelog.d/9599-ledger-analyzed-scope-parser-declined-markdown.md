<!-- section: Fixed -->
- `ripr reports gap-ledger --repo-exposure` now records an `analysis_coverage`
  disclosure (a complete whole-Rust-workspace run, its corpus identity, and a
  digest of the ledger's exact records) when the repo-exposure run was
  complete. `gate evaluate --gap-ledger` turns a still-matching disclosure into a
  `rust_workspace_complete` scope, so `baseline diff` and `baseline update
  --remove-resolved` can resolve a fixed Rust gap on the ledger path. Filtered
  or edited ledgers, limited or parser-declined runs and non-Rust paths still
  read `not_evaluated` (#9599). The review-comments Markdown summary now lists
  parser-declined files as a limitation (#9734).
