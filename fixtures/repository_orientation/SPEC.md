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
