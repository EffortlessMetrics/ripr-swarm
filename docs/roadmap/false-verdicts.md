# False-verdict ledger

Tracking issue: #9430, linked from the 0.11 tracker (#9271). Measured on main 3608bb9a
(2026-10-22) with `cargo xtask verdict-corpus check-all`.

| Language | False | Cases |
| --- | --- | --- |
| Rust | 3 | 510 |
| TypeScript | 3 | 180 |
| Python | 0 | 80 |
| Perl | 1 | 60 |

Every false row is listed once in the `[[known_false]]` tables of
`fixtures/<language>-verdict-corpus/spec-coverage.toml`, each with its owning
issue. `verdict-corpus check` fails on a false row that is not listed, and on a
listed row that is no longer false, so a bless cannot absorb a new false
verdict and a fixed row cannot return unnoticed. Fix a row in the case, the
spec or the analyzer. Never exclude or reclassify it.

## Rows

| Case | Outcome | Root cause | Fix, in | Owner | Pinned by |
| --- | --- | --- | --- | --- | --- |
| `grid-flow-err-guard` (Rust) | false actionable | Truth was mislabeled: the case's test does not pin the version string, so the mutant `format!("v{}", version + 1)` survives. | Case (relabel to `partially_discriminated`) | #6578, PR #9408 | The corpus row plus the recorded surviving mutant |
| `mined-allowlist-listed-row-ftp` (Rust) | false actionable | A conditional `continue` on a const skip-list is treated as an unresolved bypass of every table row. | Analyzer: evaluate `CONST.contains(&key)` per literal row | #9409 | Row, with `mined-allowlisted-row-gopher` as the paired negative |
| `regex-syntax-max-scalar-two-byte` (Rust) | false actionable | The match-arm selector domain through a bounded caller loop is not derived. | Analyzer | #5503 | Row, with `regex-syntax-word-byte` as the negative |
| `jest-mined-private-impl-dispatcher` (TS) | false actionable | A module-private impl behind an exported dispatcher reads as a gap and the repair names a private symbol. | Analyzer: follow the exported caller | #6655 | Row |
| `jest-spec0246-ex07-then-rejection-handler` (TS) | false actionable | `.then(ok, onRejected)` can swallow a rejection, so RIPR-SPEC-0246 collects no assertion. | Analyzer, allowlist of provably rethrowing handler shapes | #8679 | Row |
| `jest-spec0246-ex09-async-foreach` (TS) | false actionable | `forEach` discards the async callback's promise. | Analyzer, runner-aware unhandled-rejection read | #8679 | Row |
| `perl-testmore-tier-far-from-boundary` (Perl) | false exposed | No row-owned predicate discriminator fact, so inputs far from the changed threshold look aligned. | Producer fact, then consumer | #8979 (perl-lsp-swarm#17237, #5562) | Row |

## Gate

- `known_false_gate` in `xtask/src/reports/verdict_corpus_known_false.rs`
  runs inside `verdict-corpus check` for a whole-corpus run, which
  `check-all` runs in CI (`rust-gates.yml`, "Verdict corpora"). A case subset
  never trips it.
- Each fix removes its entry in the same PR. The entry's removal is the
  regression test: if the row turns false again, `check` reports an unlisted
  false verdict.
