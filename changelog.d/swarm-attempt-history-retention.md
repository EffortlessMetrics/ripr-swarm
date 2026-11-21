<!-- section: Internal / CI -->
- `cargo xtask ripr-swarm attempt-ledger` preserves previously registered
  receipts whose paths normalize to the same attempt ID and refuses to
  overwrite a prior ledger with inconsistent history (#440).
