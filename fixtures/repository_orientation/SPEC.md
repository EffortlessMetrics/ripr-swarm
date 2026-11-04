# Repository orientation corpora (#2331)

Committed corpora for the cold-start repository orientation integration
proof in `xtask/src/repository_orientation.rs`. Each scenario pins one
representative recovery case from issue #2331 using immutable work-portfolio
captured inputs plus synthetic orientation expectations. The harness selects
no work, mutates no GitHub state, and never requires deleted `.ripr/goals`
machinery.

## Layout

- `corpus.json` — nine required scenarios with expected dispositions,
  limitations, and authority boundaries.
- `provenance.json` — SHA-256 binding for `corpus.json`.

## Proof

```bash
cargo test -p xtask repository_orientation -- --test-threads=1
cargo xtask current-state --check
```

The loader requires exactly the nine named scenario IDs, each once. The
receipt's development and release authority come from the current-state
producer, independently of the scenario's expected fields. Release routing
checks the requested issue against that producer's release authority and does
not override unavailable live state, historical-only inputs, or a blocked
portfolio candidate. The tests include conflicting expectations and release
issues as well as duplicate and substituted IDs.

This is a synthetic contract: the current-state producer currently pins its
repository and release authority in source. Neither these checks nor the
fixture establish that live GitHub authority was discovered or that a release
is authorized. A changed real authority requires separate reconciliation.
