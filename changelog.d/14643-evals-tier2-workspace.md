<!-- section: Internal / CI -->
- Scaffold the Tier 2 `evals/` Cargo workspace (`evals/Cargo.toml` and
  `evals/ripr-eval-smoke`), excluded from the root workspace via `exclude = ["evals"]`
  and registered in repository workspace/dependency/file/process policies (#14643).
