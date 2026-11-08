<!-- section: Internal / CI -->
- LSP refresh prepare and commit now share one immutable `Arc`-backed diagnostic generation across snapshot inspection, refresh planning, rollback and committed state, and apply quarantine as a sparse per-URI overlay instead of deep-cloning the full diagnostic map; published diagnostic bytes are unchanged (#12369, advances #4998).
