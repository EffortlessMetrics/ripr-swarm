<!-- section: Fixed -->
- `ripr reports gap-ledger --repo-exposure` now records an `analysis_coverage`
  disclosure (a complete whole-Rust-workspace run of a producer-issued artifact,
  its corpus identity, and a digest of every record) when the repo-exposure run
  was complete. `gate evaluate --gap-ledger` turns a still-matching, fresh
  disclosure (the corpus hash must equal the gated tree's) into a
  `rust_workspace_complete` scope, which a PR-guidance document cannot carry, so
  `baseline diff` and `baseline update --remove-resolved` can resolve a fixed Rust gap on the ledger path. Filtered,
  edited or stale ledgers, limited or parser-declined runs, non-Rust paths and
  paths outside the producer's actual production-file membership (including
  symlink aliases and evidence-only sources) still read `not_evaluated` (#9599).
  Gate admission verifies current membership as well as corpus content. The review-comments Markdown summary now lists
  parser-declined files as a limitation (#9734).
