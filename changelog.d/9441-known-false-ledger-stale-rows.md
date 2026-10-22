<!-- section: Internal / CI -->
- Removes three `known_false` ledger rows (`shape-op-add-impl`, `shape-drop-impl-side-effect`, `shape-dyn-trait-method-exact`) that the verdict corpus no longer scores as false verdicts, so the ledger wiring test passes again on main (#9441).
