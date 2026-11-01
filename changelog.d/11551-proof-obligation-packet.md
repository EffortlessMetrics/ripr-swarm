<!-- section: Added -->
- Agent seam packets for a Rust relational boundary (`<`, `<=`, `>`, `>=`) now carry an optional, advisory `proof_obligation` object: the changed predicate, symbolic witness inputs (just below, equal, just above), a Kani/property skeleton and its limits. It changes no class, `task` or edit cage; `schema_version` stays `0.5` (#11551).
