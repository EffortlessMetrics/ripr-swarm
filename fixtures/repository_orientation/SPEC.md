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

Release routing uses the repository's shared authority constants, independently
of corpus expectations. A conflicting release issue is rejected as an input
error. A matching release hint does not override historical refusal, unavailable
captured GitHub state, or a selected candidate's overlap, partial acceptance,
blocked or complete posture. With no candidate selected, a matching release hint
can identify the source release route after the historical and live-state guards.
This synthetic route is orientation information, not release authorization.

Focused counterexamples also combine the matching release hint with historical,
unavailable-live, overlapping, blocked and partially landed inputs. Positive
controls retain the source-release-only and eligible-candidate routes. Stale or
unavailable generated inventory remains an explicit limitation rather than a new
routing veto, consistent with the committed stale-inventory scenario.

```bash
cargo test -p xtask repository_orientation -- --test-threads=1
cargo xtask current-state --check
```
